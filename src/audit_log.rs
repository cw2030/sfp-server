use sqlx::mysql::MySqlPool;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

#[derive(Debug, Clone)]
pub struct AuditEvent {
    pub session_id: String,
    pub username: Option<String>,
    pub client_ip: String,
    pub op_type: String,
    pub file_path: Option<String>,
    pub file_size: u64,
    pub transferred_bytes: u64,
    pub speed_bps: u64,
    pub result: String,
    pub msg: Option<String>,
}

impl AuditEvent {
    pub fn new(
        session_id: &str,
        username: Option<String>,
        client_ip: &str,
        op_type: &str,
        result: &str,
    ) -> Self {
        Self {
            session_id: session_id.to_string(),
            username,
            client_ip: client_ip.to_string(),
            op_type: op_type.to_string(),
            file_path: None,
            file_size: 0,
            transferred_bytes: 0,
            speed_bps: 0,
            result: result.to_string(),
            msg: None,
        }
    }
}

/// 启动异步审计日志消费任务：内存队列 -> MySQL 批量写入
pub fn spawn(pool: MySqlPool) -> (UnboundedSender<AuditEvent>, tokio::task::JoinHandle<()>) {
    let (tx, rx) = mpsc::unbounded_channel();
    let handle = tokio::spawn(consume(pool, rx));
    (tx, handle)
}

async fn consume(pool: MySqlPool, mut rx: UnboundedReceiver<AuditEvent>) {
    let mut batch: Vec<AuditEvent> = Vec::with_capacity(64);
    loop {
        // 等待第一条，最多 1 秒刷一次
        match tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv()).await {
            Ok(Some(ev)) => batch.push(ev),
            Ok(None) => {
                flush(&pool, &mut batch).await;
                break;
            }
            Err(_) => {}
        }

        while batch.len() < 64 {
            match rx.try_recv() {
                Ok(ev) => batch.push(ev),
                Err(_) => break,
            }
        }
        flush(&pool, &mut batch).await;
    }
}

async fn flush(pool: &MySqlPool, batch: &mut Vec<AuditEvent>) {
    if batch.is_empty() {
        return;
    }
    for ev in batch.iter() {
        tracing::info!(
            session_id = %ev.session_id,
            username = ?ev.username,
            client_ip = %ev.client_ip,
            op_type = %ev.op_type,
            file_path = ?ev.file_path,
            file_size = ev.file_size,
            transferred_bytes = ev.transferred_bytes,
            speed_bps = ev.speed_bps,
            result = %ev.result,
            msg = ?ev.msg,
            "audit"
        );
    }

    // MySQL 故障时不阻塞 SFTP：记录告警并丢弃本批（内存队列仍会缓冲后续事件）
    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::warn!(error = %e, count = batch.len(), "审计日志写入失败，本批丢失");
            batch.clear();
            return;
        }
    };

    for ev in batch.iter() {
        let res = sqlx::query(
            "INSERT INTO sftp_audit_log (session_id, username, client_ip, op_type, file_path, file_size, transferred_bytes, speed_bps, result, msg) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&ev.session_id)
        .bind(&ev.username)
        .bind(&ev.client_ip)
        .bind(&ev.op_type)
        .bind(&ev.file_path)
        .bind(ev.file_size as i64)
        .bind(ev.transferred_bytes as i64)
        .bind(ev.speed_bps as i64)
        .bind(&ev.result)
        .bind(&ev.msg)
        .execute(&mut *tx)
        .await;

        if let Err(e) = res {
            tracing::warn!(error = %e, "审计日志单条写入失败");
        }
    }

    if let Err(e) = tx.commit().await {
        tracing::warn!(error = %e, "审计日志提交失败");
    }
    batch.clear();
}
