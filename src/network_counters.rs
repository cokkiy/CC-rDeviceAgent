use anyhow::{Context, Result};

#[derive(Debug, Clone, Default)]
pub struct InterfaceCounterSnapshot {
    pub if_name: String,
    pub bytes_received: u64,
    pub bytes_sented: u64,
    pub unicast_packet_received: u64,
    pub unicast_packet_sented: u64,
    pub multicast_packet_received: u64,
    pub multicast_packet_sented: u64,
}

#[derive(Debug, Clone, Default)]
pub struct NetworkCounterSnapshot {
    pub datagrams_received: u64,
    pub datagrams_sent: u64,
    pub datagrams_discarded: u64,
    pub datagrams_with_errors: u64,
    pub segments_received: u64,
    pub segments_sent: u64,
    pub errors_received: u64,
    pub current_connections: u64,
    pub reset_connections: u64,
    pub interface_counters: Vec<InterfaceCounterSnapshot>,
}

pub fn collect() -> Result<NetworkCounterSnapshot> {
    let snapshot = crate::platform::context()?
        .net_stat
        .collect()
        .context("collect network counters through PAL")?;
    Ok(NetworkCounterSnapshot {
        datagrams_received: snapshot.datagrams_received,
        datagrams_sent: snapshot.datagrams_sent,
        datagrams_discarded: snapshot.datagrams_discarded,
        datagrams_with_errors: snapshot.datagrams_with_errors,
        segments_received: snapshot.segments_received,
        segments_sent: snapshot.segments_sent,
        errors_received: snapshot.errors_received,
        current_connections: snapshot.current_connections,
        reset_connections: snapshot.reset_connections,
        interface_counters: snapshot
            .interface_counters
            .into_iter()
            .map(|counter| InterfaceCounterSnapshot {
                if_name: counter.if_name,
                bytes_received: counter.bytes_received,
                bytes_sented: counter.bytes_sented,
                unicast_packet_received: counter.unicast_packet_received,
                unicast_packet_sented: counter.unicast_packet_sented,
                multicast_packet_received: counter.multicast_packet_received,
                multicast_packet_sented: counter.multicast_packet_sented,
            })
            .collect(),
    })
}
