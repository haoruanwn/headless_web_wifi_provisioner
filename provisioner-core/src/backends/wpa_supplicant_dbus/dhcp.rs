use std::net::Ipv4Addr;
use std::collections::{HashMap, VecDeque};
use tokio::net::UdpSocket;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

/// Spawn a very small DHCP responder in a tokio task.
/// - listens on 0.0.0.0:67
/// - maintains a tiny pool of addresses derived from `ap_ip` (.2 and .3)
/// - responds to DHCPDISCOVER with DHCPOFFER and DHCPREQUEST with DHCPACK
/// - returns (shutdown_sender, join_handle)
pub fn spawn_server(ap_ip: Ipv4Addr) -> (oneshot::Sender<()>, JoinHandle<()>) {
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    let handle = tokio::spawn(async move {
        if let Err(e) = run(ap_ip, shutdown_rx).await {
            log::error!("mini-dhcp server error: {:?}", e);
        }
    });

    (shutdown_tx, handle)
}

async fn run(ap_ip: Ipv4Addr, mut shutdown_rx: oneshot::Receiver<()>) -> anyhow::Result<()> {
    // Build pool: ap_ip + 1, +2 (e.g., 192.168.4.1 -> .2 and .3)
    let base = u32::from(ap_ip);
    let pool_ips: VecDeque<Ipv4Addr> = (1..=2).map(|i| Ipv4Addr::from(base + i)).collect();
    let mut free = pool_ips;
    let mut leases: HashMap<[u8;6], Ipv4Addr> = HashMap::new();

    // Bind to 0.0.0.0:67 (requires privileges on Linux)
    let sock = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 67)).await?;
    sock.set_broadcast(true)?;

    let mut buf = [0u8; 1500];

    loop {
        tokio::select! {
            biased;
            _ = &mut shutdown_rx => {
                log::info!("mini-dhcp: shutdown requested");
                break;
            }
            res = sock.recv_from(&mut buf) => {
                match res {
                    Ok((len, _addr)) => {
                        if let Some(msg_type) = parse_dhcp_msg_type(&buf[..len]) {
                            match msg_type {
                                1 => { // DHCPDISCOVER
                                    if let Some(mac) = extract_chaddr(&buf[..len]) {
                                        let offered = *leases.get(&mac).unwrap_or_else(|| {
                                            // allocate if available
                                            if let Some(ip) = free.front() {
                                                ip
                                            } else {
                                                &Ipv4Addr::UNSPECIFIED
                                            }
                                        });
                                        if offered != Ipv4Addr::UNSPECIFIED {
                                            let pkt = build_offer(&buf[..len], offered, ap_ip);
                                            let _ = sock.send_to(&pkt, (Ipv4Addr::BROADCAST, 68)).await;
                                            log::info!("mini-dhcp: OFFER {} for {:02x?}", offered, mac);
                                        } else {
                                            log::warn!("mini-dhcp: pool exhausted, cannot offer");
                                        }
                                    }
                                }
                                3 => { // DHCPREQUEST
                                    if let Some(mac) = extract_chaddr(&buf[..len]) {
                                        // if already leased, ack same; otherwise allocate
                                        let ip = leases.entry(mac).or_insert_with(|| {
                                            if let Some(ip) = free.pop_front() {
                                                ip
                                            } else {
                                                Ipv4Addr::UNSPECIFIED
                                            }
                                        }).clone();
                                        if ip != Ipv4Addr::UNSPECIFIED {
                                            let pkt = build_ack(&buf[..len], ip, ap_ip);
                                            let _ = sock.send_to(&pkt, (Ipv4Addr::BROADCAST, 68)).await;
                                            log::info!("mini-dhcp: ACK {} for {:02x?}", ip, mac);
                                        } else {
                                            log::warn!("mini-dhcp: no ip to ACK");
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    Err(e) => log::error!("mini-dhcp recv error: {:?}", e),
                }
            }
        }
    }

    Ok(())
}

// Helpers: parse DHCP message type from options (option 53). Returns None if not a DHCP packet.
fn parse_dhcp_msg_type(pkt: &[u8]) -> Option<u8> {
    if pkt.len() < 240 { return None; }
    let cookie = &pkt[236..240];
    if cookie != [0x63,0x82,0x53,0x63] { return None; }
    let mut i = 240;
    while i+1 <= pkt.len() {
        let opt = pkt[i];
        if opt == 255 { break; }
        if opt == 0 { i +=1; continue; }
        if i+1 >= pkt.len() { break; }
        let len = pkt[i+1] as usize;
        if i+2+len > pkt.len() { break; }
        if opt == 53 && len==1 { return Some(pkt[i+2]); }
        i += 2 + len;
    }
    None
}

fn extract_chaddr(pkt: &[u8]) -> Option<[u8;6]> {
    if pkt.len() < 34 { return None; }
    let mut mac = [0u8;6];
    mac.copy_from_slice(&pkt[28..34]);
    Some(mac)
}

fn build_offer(request: &[u8], yiaddr: Ipv4Addr, server_ip: Ipv4Addr) -> Vec<u8> {
    build_reply(request, yiaddr, server_ip, 2) // DHCPOFFER
}

fn build_ack(request: &[u8], yiaddr: Ipv4Addr, server_ip: Ipv4Addr) -> Vec<u8> {
    build_reply(request, yiaddr, server_ip, 5) // DHCPACK
}

fn build_reply(request: &[u8], yiaddr: Ipv4Addr, server_ip: Ipv4Addr, msg_type: u8) -> Vec<u8> {
    let mut buf = vec![0u8; 240];
    // op = 2 (reply)
    buf[0] = 2;
    // copy htype, hlen
    if request.len() >= 3 {
        buf[1] = request[1];
        buf[2] = request[2];
    }
    // xid
    if request.len() >= 8 {
        buf[4..8].copy_from_slice(&request[4..8]);
    }
    // yiaddr
    buf[16..20].copy_from_slice(&yiaddr.octets());
    // chaddr copy
    if request.len() >= 44 {
        buf[28..44].copy_from_slice(&request[28..44]);
    }
    // magic cookie
    buf[236..240].copy_from_slice(&[0x63,0x82,0x53,0x63]);

    // options: msg type, server id, lease time, subnet mask, router, end
    let mut opts = Vec::new();
    opts.push(53u8); opts.push(1u8); opts.push(msg_type);
    // server id option 54
    opts.push(54); opts.push(4); opts.extend_from_slice(&server_ip.octets());
    // subnet mask option 1 -> 255.255.255.0
    opts.push(1); opts.push(4); opts.extend_from_slice(&[255,255,255,0]);
    // router option 3
    opts.push(3); opts.push(4); opts.extend_from_slice(&server_ip.octets());
    // lease time 51 -> 3600s
    opts.push(51); opts.push(4); opts.extend_from_slice(&3600u32.to_be_bytes());
    // end
    opts.push(255);

    buf.extend_from_slice(&opts);
    buf
}
