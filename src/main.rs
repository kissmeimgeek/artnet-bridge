use structopt::StructOpt;
use std::net::{Ipv4Addr, UdpSocket, SocketAddr, ToSocketAddrs};
use artnet_protocol::{ArtCommand, PollReply};
use std::str::FromStr;

use log::{error, info, debug, trace};
use anyhow::Error;

extern crate pretty_env_logger;
extern crate serde_json;

mod config;
mod kinet;
mod utils;


fn main() -> Result<(), Error> {

    // Load configuration from command line
    let args = config::Configuration::from_args();

    pretty_env_logger::formatted_timed_builder()
        .filter(None, args.get_log_level().unwrap().to_level_filter())
        .init();
    
    if !args.config_file.is_empty() {
        let file_args = config::Configuration::read_from_file(args.config_file).unwrap();
        

    }

    let mut short_name: [u8; 18] = [0; 18];
    let mut long_name: [u8; 64] = [0; 64];

    let default_short_name = "ArtNet/KiNETBridge";
    let default_long_name = "ArtNet/KiNET Bridge v0.1.0";
    short_name.copy_from_slice(&default_short_name.as_bytes()[..18]);
    long_name[..26].copy_from_slice(&default_long_name.as_bytes()[..]);

    info!("Listening for Art-Net packets on {}", args.artnet_address);
    info!("Transmitting KiNET on {}", args.kinet_address);
    info!("Mapping universes to the following addresses:");
    info!("{:?}", args.pds_addresses);
    
    let artnet_socket = 
        UdpSocket::bind((&args.artnet_address[..], 6454))
        .expect("Could not bind to Art-Net address.");
    let kinet_socket = 
        UdpSocket::bind((&args.kinet_address[..], 6038))
        .expect("Could not bind to KiNET address.");

    let pds_addrs: Vec<SocketAddr> = args.pds_addresses.iter().map(|addr_string| {
       (&addr_string[..], 6038).to_socket_addrs().expect("Could not parse PDS address.").next().unwrap()
    }).collect();
    
    loop {
        let mut buffer = [0u8; 1024];
        let (length, addr) = artnet_socket.recv_from(&mut buffer)?;
        let command = ArtCommand::from_buffer(&buffer[..length])?;
        
        match command {
            ArtCommand::Poll(poll) => {
                debug!("Received Art-Net poll command {:?}", poll);
                
                let command = ArtCommand::PollReply(
                    Box::new( 
                        PollReply {
                            address: Ipv4Addr::from_str(&args.artnet_address)?,
                            port: 6454,
                            num_ports: utils::clone_into_array(&args.pds_addresses.len().to_le_bytes()[..2]),
                            short_name: short_name,
                            long_name: long_name,
                            ..utils::default_poll_reply()
                        }
                    )
                );
                match utils::send_artnet_command(command, &artnet_socket, &addr) {
                    Err(e) => { error!("{:?}", e); },
                    Ok(()) => {}
                }
            },
            ArtCommand::PollReply(_reply) => {
            },
            ArtCommand::Output(output) => {
                debug!("Received Art-Net output command for subnet {:?} of length {:?}", output.subnet, output.length);
                trace!("{:?}", output);

                let mut kinet_output = kinet::Output::default();
                kinet_output.data.copy_from_slice(&output.data[..512]);
                match kinet_output.serialize() {
                    Err(e) => { error!("{:?}", e); },
                    Ok(bytes) => {
                        debug!("Sending KiNET output packet to {:?}", pds_addrs[0]);
                        trace!("{:?}", bytes);
                        
                        match kinet_socket.send_to(&bytes, &pds_addrs[0]) {
                            Err(e) => { error!("{:?}", e); },
                            Ok(_count) => {}
                        }
                    }
                }
            },
            _ => {
                debug!("Received unhandled Art-Net command");
                trace!("{:?}", command);
            }
        }
    }
}