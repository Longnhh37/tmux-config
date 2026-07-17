// collectors/ports.rs
//
// Clear logic:
//   - lsof trả Err (IO error / crash) → GIỮ ports cũ
//   - lsof trả Ok(empty) 1 lần → GIỮ ports cũ (race condition khi port đang bind)
//   - lsof trả Ok(empty) 2 lần liên tiếp → clear (port thực sự đã đóng)

use crate::state::SharedState;
use netstat2::{get_sockets_info, AddressFamilyFlags, ProtocolFlags};
use std::collections::HashSet;
use std::time::Duration;

const PORTS_OF_INTEREST: &[u16] = &[
    5432,  // postgres
    6379,  // redis
    3306,  // mysql
    27017, // mongodb
    3001,  // node dev
    5001,  // python
    5173,  // vite
    8000,  // python / django
    8080,  // generic http
    8888,  // jupyter
];

const POLL_INTERVAL: Duration = Duration::from_secs(5);

/// Số lần liên tiếp lsof trả empty trước khi clear listening_ports.
const EMPTY_THRESHOLD: u32 = 2;

pub async fn run(state: SharedState) {
    let port_filter: String = PORTS_OF_INTEREST
        .iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>()
        .join(",");

    let mut consecutive_empty: u32 = 0;

    loop {
        match poll_ports(&port_filter).await {
            Err(_) => {
                // lsof fail (IO error) → GIỮ nguyên, không reset
            }
            Ok(ports) if ports.is_empty() => {
                consecutive_empty += 1;
                // Chỉ clear sau N lần liên tiếp
                if should_clear(consecutive_empty) {
                    let mut s = state.write().await;
                    if !s.listening_ports.is_empty() {
                        s.listening_ports.clear();
                    }
                }
            }
            Ok(ports) => {
                consecutive_empty = 0;
                state.write().await.listening_ports = ports;
            }
        }

        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

/// True once `lsof`/netstat has come back empty `EMPTY_THRESHOLD` times in a
/// row — i.e. we're confident the port is actually closed rather than
/// mid-bind. Pulled out of the polling loop so the hysteresis rule itself is
/// unit-testable without mocking `netstat2`.
fn should_clear(consecutive_empty: u32) -> bool {
    consecutive_empty >= EMPTY_THRESHOLD
}

async fn poll_ports(_port_filter: &str) -> anyhow::Result<HashSet<u16>> {
    // Chạy trong spawn_blocking vì đây là OS syscalls có thể block nhẹ
    let ports = tokio::task::spawn_blocking(|| {
        let mut active_ports = HashSet::new();
        let af_flags = AddressFamilyFlags::IPV4 | AddressFamilyFlags::IPV6;
        let proto_flags = ProtocolFlags::TCP;

        if let Ok(sockets) = get_sockets_info(af_flags, proto_flags) {
            for si in sockets {
                if let netstat2::ProtocolSocketInfo::Tcp(tcp) = si.protocol_socket_info {
                    if tcp.state == netstat2::TcpState::Listen {
                        active_ports.insert(tcp.local_port);
                    }
                }
            }
        }
        active_ports
    })
    .await?;

    // Chỉ giữ lại những port ta quan tâm
    let interest: HashSet<u16> = PORTS_OF_INTEREST.iter().copied().collect();
    Ok(ports.intersection(&interest).copied().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn does_not_clear_on_first_empty_reading() {
        assert!(!should_clear(1));
    }

    #[test]
    fn clears_once_threshold_reached() {
        assert!(should_clear(EMPTY_THRESHOLD));
        assert!(should_clear(EMPTY_THRESHOLD + 1));
    }

    #[test]
    fn zero_consecutive_empty_never_clears() {
        assert!(!should_clear(0));
    }

    #[test]
    fn ports_of_interest_covers_expected_databases_and_dev_servers() {
        for p in [5432u16, 6379, 3306, 27017, 3001, 5001, 5173, 8000, 8080, 8888] {
            assert!(
                PORTS_OF_INTEREST.contains(&p),
                "expected {p} to be in PORTS_OF_INTEREST"
            );
        }
    }
}
