use argon2::password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use chrono::NaiveDateTime;
use ipnet::IpNet;
use std::net::IpAddr;
use std::path::{Component, Path, PathBuf};
use std::str::FromStr;

pub fn hash_password(password: &str) -> anyhow::Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("密码哈希失败: {e}"))?;
    Ok(hash.to_string())
}

pub fn verify_password(hash: &str, password: &str) -> bool {
    match PasswordHash::new(hash) {
        Ok(parsed) => Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => false,
    }
}

/// 解析单个 IP 或 CIDR；裸 IP 自动补齐为 /32 或 /128
pub fn parse_cidr(s: &str) -> Result<IpNet, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("空的 IP/CIDR".to_string());
    }
    if let Ok(net) = IpNet::from_str(s) {
        return Ok(net);
    }
    let ip = IpAddr::from_str(s).map_err(|_| format!("非法 IP/CIDR: {s}"))?;
    Ok(IpNet::from(ip))
}

pub fn parse_cidr_list(list: &[String]) -> Result<Vec<IpNet>, String> {
    list.iter().map(|s| parse_cidr(s)).collect()
}

/// 把 SFTP 虚拟路径安全映射到用户根目录，禁止路径穿越
pub fn resolve_path(root: &Path, virtual_path: &str) -> Result<PathBuf, &'static str> {
    if virtual_path.contains('\0') {
        return Err("非法路径");
    }

    let mut depth: Vec<String> = Vec::new();
    for part in virtual_path.split('/') {
        match part {
            "" | "." => continue,
            ".." => {
                if depth.pop().is_none() {
                    return Err("非法路径: 越权访问根目录之外");
                }
            }
            other => depth.push(other.to_string()),
        }
    }

    let root_abs = root
        .canonicalize()
        .unwrap_or_else(|_| root.to_path_buf());
    let mut candidate = root_abs.clone();
    for part in &depth {
        candidate.push(part);
    }
    if !candidate.starts_with(&root_abs) {
        return Err("非法路径: 越权访问根目录之外");
    }

    // 对已存在的路径做真实路径校验，防止符号链接逃逸
    match candidate.canonicalize() {
        Ok(real) => {
            if !real.starts_with(&root_abs) {
                return Err("非法路径: 符号链接指向根目录之外");
            }
            Ok(real)
        }
        Err(_) => {
            // 目标不存在（创建场景），校验父目录
            if let Some(parent) = candidate.parent() {
                if let Ok(real_parent) = parent.canonicalize() {
                    if !real_parent.starts_with(&root_abs) {
                        return Err("非法路径: 父目录指向根目录之外");
                    }
                }
            }
            Ok(candidate)
        }
    }
}

/// 返回虚拟路径（相对根目录），用于审计
pub fn to_virtual_path(root: &Path, real: &Path) -> String {
    let root_abs = root
        .canonicalize()
        .unwrap_or_else(|_| root.to_path_buf());
    let rel: PathBuf = real.strip_prefix(&root_abs).unwrap_or(real).to_path_buf();
    let mut s = String::from("/");
    let parts: Vec<String> = rel
        .components()
        .filter_map(|c| match c {
            Component::Normal(v) => Some(v.to_string_lossy().to_string()),
            _ => None,
        })
        .collect();
    s.push_str(&parts.join("/"));
    s
}

pub fn format_dt(dt: Option<NaiveDateTime>) -> Option<String> {
    dt.map(|d| d.format("%Y-%m-%d %H:%M:%S").to_string())
}

pub fn new_session_id() -> String {
    uuid::Uuid::new_v4().to_string()
}
