use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use rocket::{
    http::Status,
    outcome::Outcome,
    request::{self, FromRequest, Request},
};

/// The request guard used for getting an IP address from a client.
#[derive(Debug, Clone)]
pub struct ClientAddr {
    /// IP address from a client.
    pub ip: IpAddr,
}

fn is_local_ip(addr: &IpAddr) -> bool {
    match addr {
        IpAddr::V4(addr) => {
            let octets = addr.octets();

            match octets {
                // --- is_private ---
                [10, ..] => true,
                [172, b, ..] if (16..=31).contains(&b) => true,
                [192, 168, ..] => true,
                // --- is_loopback ---
                [127, ..] => true,
                // --- is_link_local ---
                [169, 254, ..] => true,
                // --- is_broadcast ---
                [255, 255, 255, 255] => true,
                // --- is_documentation ---
                [192, 0, 2, _] => true,
                [198, 51, 100, _] => true,
                [203, 0, 113, _] => true,
                // --- is_unspecified ---
                [0, 0, 0, 0] => true,
                _ => false,
            }
        },
        IpAddr::V6(addr) => {
            let segments = addr.segments();

            let is_multicast = segments[0] & 0xFF00 == 0xFF00;

            if is_multicast {
                segments[0] & 0x000F != 14 // 14 means global
            } else {
                match segments {
                    // --- is_loopback ---
                    [0, 0, 0, 0, 0, 0, 0, 1] => true,
                    // --- is_unspecified ---
                    [0, 0, 0, 0, 0, 0, 0, 0] => true,
                    _ => {
                        match segments[0] & 0xFFC0 {
                            // --- is_unicast_link_local ---
                            0xFE80 => true,
                            // --- is_unicast_site_local ---
                            0xFEC0 => true,
                            _ => {
                                // --- is_unique_local ---
                                if segments[0] & 0xFE00 == 0xFC00 {
                                    true
                                } else {
                                    (segments[0] == 0x2001) && (segments[1] == 0xDB8)
                                }
                            },
                        }
                    },
                }
            }
        },
    }
}

fn from_request(request: &Request<'_>) -> Option<ClientAddr> {
    let (remote_ip, ip_is_local) = match request.remote().map(|e| e.ip()).flatten() {
        Some(ip) => {
            let ok = !is_local_ip(&ip);
            (Some(ip), ok)
        },
        None => (None, false),
    };

    if ip_is_local {
        match remote_ip {
            Some(ip) => Some(ClientAddr { ip }),
            None => unreachable!(),
        }
    } else {
        let forwarded_for_ip: Option<&str> = request.headers().get("x-forwarded-for").next(); // Only fetch the first one.

        match forwarded_for_ip {
            Some(forwarded_for_ip) => {
                let forwarded_for_ips = forwarded_for_ip.rsplit(',');

                let mut last_ip = None;

                for forwarded_for_ip in forwarded_for_ips {
                    match forwarded_for_ip.trim().parse::<IpAddr>() {
                        Ok(ip) => {
                            last_ip = Some(ip);

                            if !is_local_ip(&ip) {
                                break;
                            }
                        },
                        Err(_) => {
                            break;
                        },
                    }
                }

                match last_ip {
                    Some(ip) => Some(ClientAddr { ip }),
                    None => match request.real_ip() {
                        Some(real_ip) => Some(ClientAddr { ip: real_ip }),
                        None => remote_ip.map(|ip| ClientAddr { ip }),
                    },
                }
            },
            None => match request.real_ip() {
                Some(real_ip) => Some(ClientAddr { ip: real_ip }),
                None => remote_ip.map(|ip| ClientAddr { ip }),
            },
        }
    }
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for ClientAddr {
    type Error = ();

    async fn from_request(request: &'r Request<'_>) -> request::Outcome<Self, Self::Error> {
        match from_request(request) {
            Some(client_addr) => Outcome::Success(client_addr),
            None => Outcome::Forward(Status::BadRequest),
        }
    }
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for &'r ClientAddr {
    type Error = ();

    async fn from_request(request: &'r Request<'_>) -> request::Outcome<Self, Self::Error> {
        let cache: &Option<ClientAddr> = request.local_cache(|| from_request(request));

        match cache.as_ref() {
            Some(client_addr) => Outcome::Success(client_addr),
            None => Outcome::Forward(Status::BadRequest),
        }
    }
}

impl ClientAddr {
    /// Get an `Ipv4Addr` instance.
    pub fn get_ipv4(&self) -> Option<Ipv4Addr> {
        match &self.ip {
            IpAddr::V4(ipv4) => Some(*ipv4),
            IpAddr::V6(ipv6) => ipv6.to_ipv4(),
        }
    }

    /// Get an IPv4 string.
    pub fn get_ipv4_string(&self) -> Option<String> {
        match &self.ip {
            IpAddr::V4(ipv4) => Some(ipv4.to_string()),
            IpAddr::V6(ipv6) => ipv6.to_ipv4().map(|ipv6| ipv6.to_string()),
        }
    }

    /// Get an `Ipv6Addr` instance.
    pub fn get_ipv6(&self) -> Ipv6Addr {
        match &self.ip {
            IpAddr::V4(ipv4) => ipv4.to_ipv6_mapped(),
            IpAddr::V6(ipv6) => *ipv6,
        }
    }

    /// Get an IPv6 string.
    pub fn get_ipv6_string(&self) -> String {
        match &self.ip {
            IpAddr::V4(ipv4) => ipv4.to_ipv6_mapped().to_string(),
            IpAddr::V6(ipv6) => ipv6.to_string(),
        }
    }
}
