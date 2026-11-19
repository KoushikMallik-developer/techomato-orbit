//! Listening TCP and UDP endpoints with their owning process, from the IP Helper tables.

use crate::error::{AppError, AppResult, ErrorKind};
use crate::windows::procs::identify;
use serde::Serialize;
use std::collections::HashMap;
use std::net::{Ipv4Addr, Ipv6Addr};
use sysinfo::{ProcessesToUpdate, System};
use windows::Win32::NetworkManagement::IpHelper::{
    GetExtendedTcpTable, GetExtendedUdpTable, MIB_TCP6ROW_OWNER_PID, MIB_TCPROW_OWNER_PID,
    MIB_UDP6ROW_OWNER_PID, MIB_UDPROW_OWNER_PID, TCP_TABLE_OWNER_PID_LISTENER, UDP_TABLE_OWNER_PID,
};
use windows::Win32::Networking::WinSock::{AF_INET, AF_INET6};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Proto {
    Tcp,
    Udp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Endpoint {
    pub proto: Proto,
    pub local_addr: String,
    pub port: u16,
    pub state: Option<String>,
    pub pid: u32,
    pub created_at_ms: Option<u64>,
    pub process_name: Option<String>,
    pub path: Option<String>,
}

const NO_ERROR: u32 = 0;
const ERROR_INSUFFICIENT_BUFFER: u32 = 122;

/// Calls an IP Helper table function, growing the buffer until it fits. The buffer is `u32`s so
/// the table structs inside it are 4-byte aligned.
fn fetch(call: impl Fn(*mut core::ffi::c_void, *mut u32) -> u32) -> AppResult<Vec<u32>> {
    let mut size = 0u32;
    for _ in 0..6 {
        let mut buf = vec![0u32; (size as usize).div_ceil(4) + 64];
        let mut len = (buf.len() * 4) as u32;
        match call(buf.as_mut_ptr().cast(), &mut len) {
            NO_ERROR => return Ok(buf),
            ERROR_INSUFFICIENT_BUFFER => size = len,
            code => {
                return Err(AppError::new(
                    ErrorKind::Os,
                    format!("Could not read the network table (error {code})."),
                ))
            }
        }
    }
    Err(AppError::new(
        ErrorKind::Os,
        "The network table kept changing while it was being read.",
    ))
}

/// Reads `count` rows of type `R` that follow the leading `dwNumEntries` field.
///
/// # Safety
/// `buf` must hold a table whose rows are `R`.
unsafe fn rows<R: Copy>(buf: &[u32]) -> Vec<R> {
    let count = buf[0] as usize;
    let base = buf.as_ptr().add(1).cast::<R>();
    (0..count).map(|i| base.add(i).read_unaligned()).collect()
}

fn port_of(raw: u32) -> u16 {
    u16::from_be((raw & 0xFFFF) as u16)
}

pub fn list_endpoints() -> AppResult<Vec<Endpoint>> {
    // (proto, addr, port, state, pid)
    let mut raw: Vec<(Proto, String, u16, Option<String>, u32)> = Vec::new();
    // SAFETY: each `fetch` buffer holds the table type matching the requested class and family.
    unsafe {
        let t4 = fetch(|p, n| {
            GetExtendedTcpTable(
                Some(p),
                n,
                false,
                AF_INET.0 as u32,
                TCP_TABLE_OWNER_PID_LISTENER,
                0,
            )
        })?;
        for r in rows::<MIB_TCPROW_OWNER_PID>(&t4) {
            raw.push((
                Proto::Tcp,
                Ipv4Addr::from(r.dwLocalAddr.to_ne_bytes()).to_string(),
                port_of(r.dwLocalPort),
                Some("LISTEN".into()),
                r.dwOwningPid,
            ));
        }
        let t6 = fetch(|p, n| {
            GetExtendedTcpTable(
                Some(p),
                n,
                false,
                AF_INET6.0 as u32,
                TCP_TABLE_OWNER_PID_LISTENER,
                0,
            )
        })?;
        for r in rows::<MIB_TCP6ROW_OWNER_PID>(&t6) {
            raw.push((
                Proto::Tcp,
                Ipv6Addr::from(r.ucLocalAddr).to_string(),
                port_of(r.dwLocalPort),
                Some("LISTEN".into()),
                r.dwOwningPid,
            ));
        }
        let u4 = fetch(|p, n| {
            GetExtendedUdpTable(Some(p), n, false, AF_INET.0 as u32, UDP_TABLE_OWNER_PID, 0)
        })?;
        for r in rows::<MIB_UDPROW_OWNER_PID>(&u4) {
            raw.push((
                Proto::Udp,
                Ipv4Addr::from(r.dwLocalAddr.to_ne_bytes()).to_string(),
                port_of(r.dwLocalPort),
                None,
                r.dwOwningPid,
            ));
        }
        let u6 = fetch(|p, n| {
            GetExtendedUdpTable(Some(p), n, false, AF_INET6.0 as u32, UDP_TABLE_OWNER_PID, 0)
        })?;
        for r in rows::<MIB_UDP6ROW_OWNER_PID>(&u6) {
            raw.push((
                Proto::Udp,
                Ipv6Addr::from(r.ucLocalAddr).to_string(),
                port_of(r.dwLocalPort),
                None,
                r.dwOwningPid,
            ));
        }
    }

    // Owner details. A process we cannot inspect simply has no path or creation time.
    let mut info: HashMap<u32, (Option<String>, Option<String>)> = HashMap::new();
    if let Ok(mut sys) = System::new() {
        sys.refresh_processes(ProcessesToUpdate::All, true);
        for (pid, p) in sys.processes() {
            info.insert(
                pid.as_u32(),
                (
                    Some(p.name().to_string_lossy().into_owned()),
                    p.exe().map(|e| e.to_string_lossy().into_owned()),
                ),
            );
        }
    }
    let out: Vec<Endpoint> = raw
        .into_iter()
        .map(|(proto, local_addr, port, state, pid)| {
            let (process_name, path) = info.get(&pid).cloned().unwrap_or((None, None));
            Endpoint {
                proto,
                local_addr,
                port,
                state,
                pid,
                created_at_ms: identify(pid).map(|i| i.created_at_ms),
                process_name,
                path,
            }
        })
        .collect();
    Ok(unique_rows(out))
}

/// Sorted by port, with one row per (protocol, address, port, owner): the UI uses those four as
/// its row key.
fn unique_rows(mut rows: Vec<Endpoint>) -> Vec<Endpoint> {
    rows.sort_by(|a, b| {
        (a.port, a.proto as u8, &a.local_addr, a.pid).cmp(&(
            b.port,
            b.proto as u8,
            &b.local_addr,
            b.pid,
        ))
    });
    rows.dedup_by(|b, a| {
        a.proto == b.proto && a.local_addr == b.local_addr && a.port == b.port && a.pid == b.pid
    });
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{TcpListener, UdpSocket};

    fn mine(list: &[Endpoint], proto: Proto, port: u16) -> Option<&Endpoint> {
        list.iter()
            .find(|e| e.proto == proto && e.port == port && e.pid == std::process::id())
    }

    #[test]
    fn a_tcp_listener_is_listed_with_this_process_as_owner() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let list = list_endpoints().unwrap();
        let e = mine(&list, Proto::Tcp, port).expect("our listener is listed");
        assert_eq!(e.local_addr, "127.0.0.1");
        assert_eq!(e.state.as_deref(), Some("LISTEN"));
        assert!(!e.process_name.as_deref().unwrap_or("").is_empty());
        assert_eq!(
            e.created_at_ms,
            identify(std::process::id()).map(|i| i.created_at_ms)
        );
    }

    #[test]
    fn a_udp_socket_is_listed_without_a_state() {
        let u = UdpSocket::bind("127.0.0.1:0").unwrap();
        let port = u.local_addr().unwrap().port();
        let list = list_endpoints().unwrap();
        let e = mine(&list, Proto::Udp, port).expect("our socket is listed");
        assert_eq!(e.state, None);
    }

    #[test]
    fn an_ipv6_listener_is_listed_when_the_machine_has_ipv6() {
        let Ok(l) = TcpListener::bind("[::1]:0") else {
            return;
        };
        let port = l.local_addr().unwrap().port();
        let list = list_endpoints().unwrap();
        let e = mine(&list, Proto::Tcp, port).expect("the ipv6 listener is listed");
        assert_eq!(e.local_addr, "::1");
    }

    #[test]
    fn rows_owned_by_unreadable_processes_do_not_break_the_list() {
        let list = list_endpoints().unwrap();
        assert!(!list.is_empty());
        // Every row is well-formed whether or not its owner could be inspected.
        for e in &list {
            assert!(e.port > 0 || e.proto == Proto::Udp);
            if e.path.is_none() {
                assert!(e.pid != std::process::id());
            }
        }
    }

    #[test]
    fn sockets_of_one_process_on_the_same_address_and_port_become_one_row() {
        // mDNS-style: two sockets, same proto/address/port/owner, different state or details.
        let a = Endpoint {
            proto: Proto::Udp,
            local_addr: "0.0.0.0".into(),
            port: 5353,
            state: None,
            pid: 7,
            created_at_ms: Some(1),
            process_name: Some("chrome.exe".into()),
            path: None,
        };
        let b = Endpoint {
            path: Some("C:/x".into()),
            ..a.clone()
        };
        let other_pid = Endpoint {
            pid: 8,
            ..a.clone()
        };
        let rows = unique_rows(vec![a.clone(), other_pid, b]);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows.iter().filter(|r| r.pid == 7).count(), 1);
    }
}
