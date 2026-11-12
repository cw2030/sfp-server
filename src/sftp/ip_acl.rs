use ipnet::IpNet;
use std::net::IpAddr;

/// 白名单为空 = 不校验；否则必须命中任意一条 CIDR
pub fn ip_allowed(ip: IpAddr, whitelist: &[IpNet]) -> bool {
    whitelist.is_empty() || whitelist.iter().any(|net| net.contains(&ip))
}
