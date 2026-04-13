//! Hex-encoded `local_address` / `rem_address` fields from Linux [`/proc/net/tcp`](https://man7.org/linux/man-pages/man5/proc.5.html)
//! and `/proc/net/tcp6`. Portable pure parsing (safe to call on any target).

/// Parse `ip_hex:port_hex` into a human-readable IP and host-order port.
///
/// Supports IPv4 (8 hex nibbles for address) and IPv6 (32 hex nibbles), matching
/// the kernel's `/proc/net/tcp*` format.
#[must_use]
pub fn parse_hex_ip_port(s: &str) -> Option<(String, u16)> {
    let (addr_hex, port_hex) = s.split_once(':')?;
    if addr_hex.len() == 8 {
        let addr = u32::from_str_radix(addr_hex, 16).ok()?;
        let ip = format!(
            "{}.{}.{}.{}",
            addr & 0xff,
            (addr >> 8) & 0xff,
            (addr >> 16) & 0xff,
            (addr >> 24) & 0xff,
        );
        let port = u16::from_str_radix(port_hex, 16).ok()?;
        Some((ip, port))
    } else if addr_hex.len() == 32 {
        let mut b = [0u8; 16];
        for (i, byte) in b.iter_mut().enumerate() {
            *byte = u8::from_str_radix(addr_hex.get(i * 2..i * 2 + 2)?, 16).ok()?;
        }
        let ip = std::net::Ipv6Addr::from(b).to_string();
        let port = u16::from_str_radix(port_hex, 16).ok()?;
        Some((ip, port))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ipv4_loopback_ssh() {
        //0100007F:0016 = 127.0.0.1:22 (port0x0016 = 22)
        let (ip, port) = parse_hex_ip_port("0100007F:0016").expect("parse");
        assert_eq!(ip, "127.0.0.1");
        assert_eq!(port, 22);
    }

    #[test]
    fn ipv6_mapped_or_full() {
        let s = "00000000000000000000000000000000:0000";
        let (ip, port) = parse_hex_ip_port(s).expect("parse");
        assert_eq!(ip, "::");
        assert_eq!(port, 0);
    }
}
