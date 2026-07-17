// collectors/ports.rs

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
            }
            Ok(ports) if ports.is_empty() => {
                consecutive_empty += 1;
                if consecutive_empty >= EMPTY_THRESHOLD {
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

    let interest: HashSet<u16> = PORTS_OF_INTEREST.iter().copied().collect();
    Ok(ports.intersection(&interest).copied().collect())
}
