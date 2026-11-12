-- 0001_init.sql
CREATE TABLE IF NOT EXISTS `sftp_user` (
  `id` BIGINT PRIMARY KEY AUTO_INCREMENT,
  `username` VARCHAR(64) NOT NULL UNIQUE COMMENT 'sftp登录账号',
  `password_hash` VARCHAR(256) NULL COMMENT 'argon2id哈希，NULL=仅公钥登录',
  `public_key` TEXT NULL COMMENT 'ssh公钥字符串',
  `root_dir` VARCHAR(512) NOT NULL COMMENT '用户Linux根目录，绝对路径',
  `upload_speed` BIGINT NOT NULL DEFAULT 0 COMMENT '上传限速 byte/s，0=不限速',
  `download_speed` BIGINT NOT NULL DEFAULT 0 COMMENT '下载限速 byte/s，0=不限速',
  `enabled` TINYINT(1) NOT NULL DEFAULT 1 COMMENT '1启用，0禁用',
  `created_at` DATETIME DEFAULT CURRENT_TIMESTAMP,
  `updated_at` DATETIME DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  INDEX idx_username (username)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE IF NOT EXISTS `sftp_user_ip_whitelist` (
  `id` BIGINT PRIMARY KEY AUTO_INCREMENT,
  `user_id` BIGINT NOT NULL,
  `cidr` VARCHAR(64) NOT NULL COMMENT 'IP或CIDR，例：192.168.1.10/32、10.0.0.0/24',
  `created_at` DATETIME DEFAULT CURRENT_TIMESTAMP,
  FOREIGN KEY (user_id) REFERENCES sftp_user(id) ON DELETE CASCADE,
  UNIQUE KEY uk_user_cidr (user_id, cidr)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE IF NOT EXISTS `sftp_global_config` (
  `id` BIGINT PRIMARY KEY AUTO_INCREMENT,
  `cfg_key` VARCHAR(64) NOT NULL UNIQUE,
  `cfg_value` TEXT NULL,
  `remark` VARCHAR(255) NULL,
  `updated_at` DATETIME DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

INSERT IGNORE INTO `sftp_global_config` (`cfg_key`, `cfg_value`, `remark`) VALUES
  ('global_ip_whitelist', '[]', '全局IP白名单，JSON数组，空数组=不校验'),
  ('max_global_conn', '0', '全局最大并发连接，0=不限制'),
  ('max_user_conn', '0', '单用户最大并发会话，0=不限制'),
  ('idle_timeout_sec', '600', '空闲超时秒数');

CREATE TABLE IF NOT EXISTS `sftp_audit_log` (
  `id` BIGINT PRIMARY KEY AUTO_INCREMENT,
  `session_id` VARCHAR(64) NOT NULL COMMENT '会话uuid',
  `username` VARCHAR(64) NULL COMMENT '登录失败时为空',
  `client_ip` VARCHAR(64) NOT NULL,
  `op_type` VARCHAR(32) NOT NULL COMMENT 'login / upload / download / rm / rename / mkdir / disconnect',
  `file_path` VARCHAR(1024) NULL,
  `file_size` BIGINT DEFAULT 0 COMMENT '文件总大小',
  `transferred_bytes` BIGINT DEFAULT 0 COMMENT '本次传输字节',
  `speed_bps` BIGINT DEFAULT 0 COMMENT '平均byte/s',
  `result` VARCHAR(16) NOT NULL COMMENT 'success / fail',
  `msg` TEXT NULL COMMENT '备注、失败原因',
  `created_at` DATETIME DEFAULT CURRENT_TIMESTAMP,
  INDEX idx_session (session_id),
  INDEX idx_username (username),
  INDEX idx_created_at (created_at),
  INDEX idx_op_type (op_type)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE IF NOT EXISTS `admin_user` (
  `id` BIGINT PRIMARY KEY AUTO_INCREMENT,
  `admin_name` VARCHAR(64) NOT NULL UNIQUE,
  `password_hash` VARCHAR(256) NOT NULL,
  `enabled` TINYINT(1) NOT NULL DEFAULT 1,
  `created_at` DATETIME DEFAULT CURRENT_TIMESTAMP,
  `updated_at` DATETIME DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;
