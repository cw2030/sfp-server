use crate::audit_log::AuditEvent;
use crate::sftp::limiter::RateLimiter;
use crate::utils::resolve_path;
use russh_sftp::protocol::{
    Attrs, Data, File, FileAttributes, Handle, Name, OpenFlags, Status, StatusCode, Version,
};
use russh_sftp::server::StatusReply;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::sync::mpsc::UnboundedSender;

const MAX_READ_LEN: u32 = 256 * 1024;

struct FileHandle {
    file: tokio::fs::File,
    real_path: PathBuf,
    virtual_path: String,
    writing: bool,
    bytes: u64,
    started: Instant,
}

struct DirHandle {
    entries: Vec<File>,
    read_done: bool,
}

pub struct SftpSession {
    session_id: String,
    username: String,
    client_ip: String,
    root: PathBuf,
    upload: RateLimiter,
    download: RateLimiter,
    audit_tx: UnboundedSender<AuditEvent>,
    /// 只读用户禁止所有写操作（open 写、remove、mkdir、rmdir、rename、setstat 等）
    read_only: bool,
    version_seen: bool,
    files: HashMap<String, FileHandle>,
    dirs: HashMap<String, DirHandle>,
}

impl SftpSession {
    pub fn new(
        session_id: String,
        username: String,
        client_ip: String,
        root: PathBuf,
        upload_speed: u64,
        download_speed: u64,
        read_only: bool,
        audit_tx: UnboundedSender<AuditEvent>,
    ) -> Self {
        Self {
            session_id,
            username,
            client_ip,
            root,
            upload: RateLimiter::new(upload_speed),
            download: RateLimiter::new(download_speed),
            audit_tx,
            read_only,
            version_seen: false,
            files: HashMap::new(),
            dirs: HashMap::new(),
        }
    }

    /// 只读用户尝试写操作时返回 PermissionDenied，并记录审计
    fn deny_write(&self, op: &str, path: Option<&str>) -> StatusReply {
        let mut ev = AuditEvent::new(
            &self.session_id,
            Some(self.username.clone()),
            &self.client_ip,
            op,
            "fail",
        );
        ev.file_path = path.map(|p| p.to_string());
        ev.msg = Some("只读用户禁止该操作".to_string());
        let _ = self.audit_tx.send(ev);
        StatusReply::new(StatusCode::PermissionDenied)
            .with_message("只读用户不允许写操作")
    }

    fn map(&self, virtual_path: &str) -> Result<PathBuf, StatusReply> {
        resolve_path(&self.root, virtual_path).map_err(|msg| {
            StatusReply::new(StatusCode::PermissionDenied).with_message(msg)
        })
    }

    fn io_err(e: std::io::Error) -> StatusReply {
        let code = match e.kind() {
            std::io::ErrorKind::NotFound => StatusCode::NoSuchFile,
            std::io::ErrorKind::PermissionDenied => StatusCode::PermissionDenied,
            _ => StatusCode::Failure,
        };
        StatusReply::new(code).with_message(e.to_string())
    }

    fn audit(
        &self,
        op_type: &str,
        result: &str,
        file_path: Option<String>,
        msg: Option<String>,
    ) {
        let mut ev = AuditEvent::new(
            &self.session_id,
            Some(self.username.clone()),
            &self.client_ip,
            op_type,
            result,
        );
        ev.file_path = file_path;
        ev.msg = msg;
        let _ = self.audit_tx.send(ev);
    }

    fn ok(id: u32) -> Status {
        Status {
            id,
            status_code: StatusCode::Ok,
            error_message: "Ok".to_string(),
            language_tag: "en-US".to_string(),
        }
    }

    /// 规范化虚拟路径（realpath 用）
    fn normalize_virtual(path: &str) -> String {
        let mut parts: Vec<&str> = Vec::new();
        for part in path.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    parts.pop();
                }
                other => parts.push(other),
            }
        }
        format!("/{}", parts.join("/"))
    }
}

impl russh_sftp::server::Handler for SftpSession {
    type Error = StatusReply;

    fn unimplemented(&self) -> Self::Error {
        StatusReply::new(StatusCode::OpUnsupported)
    }

    async fn init(
        &mut self,
        _version: u32,
        _extensions: HashMap<String, String>,
    ) -> Result<Version, Self::Error> {
        if self.version_seen {
            return Err(StatusReply::new(StatusCode::ConnectionLost)
                .with_message("duplicate SSH_FXP_VERSION"));
        }
        self.version_seen = true;
        Ok(Version::new())
    }

    async fn open(
        &mut self,
        id: u32,
        filename: String,
        pflags: OpenFlags,
        _attrs: FileAttributes,
    ) -> Result<Handle, Self::Error> {
        let writing = pflags
            .intersects(OpenFlags::WRITE | OpenFlags::APPEND | OpenFlags::CREATE | OpenFlags::TRUNCATE);
        if writing && self.read_only {
            return Err(self.deny_write("open", Some(&filename)));
        }
        let real = self.map(&filename)?;
        let std_opts: std::fs::OpenOptions = pflags.into();
        let mut opts = tokio::fs::OpenOptions::from(std_opts);
        let file = opts.open(&real).await.map_err(Self::io_err)?;

        let writing = pflags
            .intersects(OpenFlags::WRITE | OpenFlags::APPEND | OpenFlags::CREATE | OpenFlags::TRUNCATE);
        let handle = uuid::Uuid::new_v4().to_string();
        self.files.insert(
            handle.clone(),
            FileHandle {
                file,
                real_path: real,
                virtual_path: filename,
                writing,
                bytes: 0,
                started: Instant::now(),
            },
        );
        Ok(Handle { id, handle })
    }

    async fn close(&mut self, id: u32, handle: String) -> Result<Status, Self::Error> {
        if let Some(mut fh) = self.files.remove(&handle) {
            let _ = fh.file.flush().await;
            let _ = fh.file.shutdown().await;
            let elapsed = fh.started.elapsed().as_secs_f64().max(0.001);
            let mut ev = AuditEvent::new(
                &self.session_id,
                Some(self.username.clone()),
                &self.client_ip,
                if fh.writing { "upload" } else { "download" },
                "success",
            );
            ev.file_path = Some(fh.virtual_path.clone());
            ev.transferred_bytes = fh.bytes;
            ev.speed_bps = (fh.bytes as f64 / elapsed) as u64;
            ev.file_size = if fh.writing {
                tokio::fs::metadata(&fh.real_path)
                    .await
                    .map(|m| m.len())
                    .unwrap_or(fh.bytes)
            } else {
                tokio::fs::metadata(&fh.real_path)
                    .await
                    .map(|m| m.len())
                    .unwrap_or(0)
            };
            let _ = self.audit_tx.send(ev);
        }
        Ok(Self::ok(id))
    }

    async fn read(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        len: u32,
    ) -> Result<Data, Self::Error> {
        let len = len.min(MAX_READ_LEN) as usize;
        let (mut buf, n) = {
            let fh = self
                .files
                .get_mut(&handle)
                .ok_or_else(|| StatusReply::new(StatusCode::Failure).with_message("无效的文件句柄"))?;
            fh.file
                .seek(std::io::SeekFrom::Start(offset))
                .await
                .map_err(Self::io_err)?;
            let mut buf = vec![0u8; len];
            let n = fh.file.read(&mut buf).await.map_err(Self::io_err)?;
            fh.bytes += n as u64;
            (buf, n)
        };

        if n == 0 {
            return Err(StatusReply::new(StatusCode::Eof));
        }
        self.download.throttle(n as u64).await;
        buf.truncate(n);
        Ok(Data { id, data: buf })
    }

    async fn write(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        data: Vec<u8>,
    ) -> Result<Status, Self::Error> {
        self.upload.throttle(data.len() as u64).await;
        let fh = self
            .files
            .get_mut(&handle)
            .ok_or_else(|| StatusReply::new(StatusCode::Failure).with_message("无效的文件句柄"))?;
        fh.file
            .seek(std::io::SeekFrom::Start(offset))
            .await
            .map_err(Self::io_err)?;
        fh.file.write_all(&data).await.map_err(Self::io_err)?;
        fh.bytes += data.len() as u64;
        Ok(Self::ok(id))
    }

    async fn lstat(&mut self, id: u32, path: String) -> Result<Attrs, Self::Error> {
        let real = self.map(&path)?;
        let meta = tokio::fs::symlink_metadata(&real).await.map_err(Self::io_err)?;
        Ok(Attrs {
            id,
            attrs: FileAttributes::from(&meta),
        })
    }

    async fn stat(&mut self, id: u32, path: String) -> Result<Attrs, Self::Error> {
        let real = self.map(&path)?;
        let meta = tokio::fs::metadata(&real).await.map_err(Self::io_err)?;
        Ok(Attrs {
            id,
            attrs: FileAttributes::from(&meta),
        })
    }

    async fn fstat(&mut self, id: u32, handle: String) -> Result<Attrs, Self::Error> {
        let fh = self
            .files
            .get(&handle)
            .ok_or_else(|| StatusReply::new(StatusCode::Failure).with_message("无效的文件句柄"))?;
        let meta = fh.file.metadata().await.map_err(Self::io_err)?;
        Ok(Attrs {
            id,
            attrs: FileAttributes::from(&meta),
        })
    }

    async fn setstat(
        &mut self,
        id: u32,
        path: String,
        attrs: FileAttributes,
    ) -> Result<Status, Self::Error> {
        if self.read_only {
            return Err(self.deny_write("setstat", Some(&path)));
        }
        let real = self.map(&path)?;
        if let Some(size) = attrs.size {
            let f = tokio::fs::OpenOptions::new()
                .write(true)
                .open(&real)
                .await
                .map_err(Self::io_err)?;
            f.set_len(size).await.map_err(Self::io_err)?;
        }
        #[cfg(unix)]
        if let Some(mode) = attrs.permissions {
            use std::os::unix::fs::PermissionsExt;
            tokio::fs::set_permissions(&real, std::fs::Permissions::from_mode(mode))
                .await
                .map_err(Self::io_err)?;
        }
        Ok(Self::ok(id))
    }

    async fn fsetstat(
        &mut self,
        id: u32,
        handle: String,
        attrs: FileAttributes,
    ) -> Result<Status, Self::Error> {
        if self.read_only {
            return Err(self.deny_write("fsetstat", None));
        }
        let fh = self
            .files
            .get_mut(&handle)
            .ok_or_else(|| StatusReply::new(StatusCode::Failure).with_message("无效的文件句柄"))?;
        if let Some(size) = attrs.size {
            fh.file.set_len(size).await.map_err(Self::io_err)?;
        }
        #[cfg(unix)]
        if let Some(mode) = attrs.permissions {
            use std::os::unix::fs::PermissionsExt;
            tokio::fs::set_permissions(&fh.real_path, std::fs::Permissions::from_mode(mode))
                .await
                .map_err(Self::io_err)?;
        }
        Ok(Self::ok(id))
    }

    async fn opendir(&mut self, id: u32, path: String) -> Result<Handle, Self::Error> {
        let real = self.map(&path)?;
        let meta = tokio::fs::metadata(&real).await.map_err(Self::io_err)?;
        if !meta.is_dir() {
            return Err(StatusReply::new(StatusCode::Failure).with_message("不是目录"));
        }

        let mut entries = Vec::new();
        let mut rd = tokio::fs::read_dir(&real).await.map_err(Self::io_err)?;
        while let Some(entry) = rd.next_entry().await.map_err(Self::io_err)? {
            let name = entry.file_name().to_string_lossy().to_string();
            match entry.metadata().await {
                Ok(meta) => entries.push(File::new(name, FileAttributes::from(&meta))),
                Err(_) => entries.push(File::new(name, FileAttributes::default())),
            }
        }

        let handle = uuid::Uuid::new_v4().to_string();
        self.dirs.insert(
            handle.clone(),
            DirHandle {
                entries,
                read_done: false,
            },
        );
        Ok(Handle { id, handle })
    }

    async fn readdir(&mut self, id: u32, handle: String) -> Result<Name, Self::Error> {
        let dh = self
            .dirs
            .get_mut(&handle)
            .ok_or_else(|| StatusReply::new(StatusCode::Failure).with_message("无效的目录句柄"))?;
        if dh.read_done {
            return Err(StatusReply::new(StatusCode::Eof));
        }
        dh.read_done = true;
        Ok(Name {
            id,
            files: std::mem::take(&mut dh.entries),
        })
    }

    async fn realpath(&mut self, id: u32, path: String) -> Result<Name, Self::Error> {
        Ok(Name {
            id,
            files: vec![File::dummy(Self::normalize_virtual(&path))],
        })
    }

    async fn readlink(&mut self, id: u32, path: String) -> Result<Name, Self::Error> {
        let real = self.map(&path)?;
        let target = tokio::fs::read_link(&real).await.map_err(Self::io_err)?;
        Ok(Name {
            id,
            files: vec![File::dummy(target.to_string_lossy().to_string())],
        })
    }

    async fn remove(&mut self, id: u32, filename: String) -> Result<Status, Self::Error> {
        if self.read_only {
            return Err(self.deny_write("remove", Some(&filename)));
        }
        let real = self.map(&filename)?;
        match tokio::fs::remove_file(&real).await {
            Ok(()) => {
                self.audit("rm", "success", Some(filename), None);
                Ok(Self::ok(id))
            }
            Err(e) => {
                self.audit("rm", "fail", Some(filename), Some(e.to_string()));
                Err(Self::io_err(e))
            }
        }
    }

    async fn mkdir(
        &mut self,
        id: u32,
        path: String,
        _attrs: FileAttributes,
    ) -> Result<Status, Self::Error> {
        if self.read_only {
            return Err(self.deny_write("mkdir", Some(&path)));
        }
        let real = self.map(&path)?;
        match tokio::fs::create_dir(&real).await {
            Ok(()) => {
                self.audit("mkdir", "success", Some(path), None);
                Ok(Self::ok(id))
            }
            Err(e) => {
                self.audit("mkdir", "fail", Some(path), Some(e.to_string()));
                Err(Self::io_err(e))
            }
        }
    }

    async fn rmdir(&mut self, id: u32, path: String) -> Result<Status, Self::Error> {
        if self.read_only {
            return Err(self.deny_write("rmdir", Some(&path)));
        }
        let real = self.map(&path)?;
        match tokio::fs::remove_dir(&real).await {
            Ok(()) => {
                self.audit("rm", "success", Some(path), Some("rmdir".into()));
                Ok(Self::ok(id))
            }
            Err(e) => {
                self.audit("rm", "fail", Some(path), Some(e.to_string()));
                Err(Self::io_err(e))
            }
        }
    }

    async fn rename(
        &mut self,
        id: u32,
        oldpath: String,
        newpath: String,
    ) -> Result<Status, Self::Error> {
        if self.read_only {
            return Err(self.deny_write(
                "rename",
                Some(&format!("{oldpath} -> {newpath}")),
            ));
        }
        let old_real = self.map(&oldpath)?;
        let new_real = self.map(&newpath)?;
        match tokio::fs::rename(&old_real, &new_real).await {
            Ok(()) => {
                self.audit(
                    "rename",
                    "success",
                    Some(format!("{oldpath} -> {newpath}")),
                    None,
                );
                Ok(Self::ok(id))
            }
            Err(e) => {
                self.audit(
                    "rename",
                    "fail",
                    Some(format!("{oldpath} -> {newpath}")),
                    Some(e.to_string()),
                );
                Err(Self::io_err(e))
            }
        }
    }

    async fn extended(
        &mut self,
        _id: u32,
        request: String,
        _data: Vec<u8>,
    ) -> Result<russh_sftp::protocol::Packet, Self::Error> {
        Err(StatusReply::new(StatusCode::OpUnsupported)
            .with_message(format!("不支持的扩展请求: {request}")))
    }
}
