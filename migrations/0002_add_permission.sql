-- 为 SFTP 用户增加读写权限控制
-- rw = 可读可写（默认，兼容老用户）
-- r  = 只读，禁止所有写操作（上传、删除、重命名、mkdir、rmdir、setstat 等）
ALTER TABLE `sftp_user`
    ADD COLUMN `permission` VARCHAR(8) NOT NULL DEFAULT 'rw'
        COMMENT 'rw=可读可写, r=只读' AFTER `enabled`;
