use std;
use std::collections::{RingBuf};
use std::io::IoResult;
pub use self::sequence::SequenceNr;

mod sequence;

struct PacketHeader {
    seq: SequenceNr,
    ack: SequenceNr,
}

impl PacketHeader {
    fn write_to<W: Writer>(&self, w: &mut W) -> IoResult<()> {
        try!(w.write_u8(self.seq));
        try!(w.write_u8(self.ack));
        Ok(())
    }

    fn read_from<R: Reader>(r: &mut R) -> IoResult<PacketHeader> {
        Ok(PacketHeader {
            seq: try!(r.read_u8()),
            ack: try!(r.read_u8())
        })
    }
}

struct PacketInfo {
    header: PacketHeader,
    sent_time: f64,
}

pub struct NetChannel {
    incoming_datagrams: Receiver<Vec<u8>>,
    outgoing_datagrams: Sender<Vec<u8>>,

    incoming_seq: SequenceNr,
    outgoing_seq: SequenceNr,
    outgoing_seq_acked: SequenceNr,

    inflight: RingBuf<PacketInfo>,

    latency: f64,
    dropped_packets: u64
}

#[deriving(Show)]
pub enum RecvError {
    TaskDied
}

impl NetChannel {
    pub fn new_from_channels(incoming_datagrams: Receiver<Vec<u8>>,
                             outgoing_datagrams: Sender<Vec<u8>>) -> NetChannel {
        NetChannel {
            incoming_datagrams: incoming_datagrams,
            outgoing_datagrams: outgoing_datagrams,

            incoming_seq: -1,
            outgoing_seq: 0,
            outgoing_seq_acked: -1,

            inflight: RingBuf::new(),

            latency: 0.,
            dropped_packets: 0
        }
    }

    pub fn transmit(&mut self, msg: &[u8]) -> SequenceNr {
        let header = self.create_header();

        // TODO: This allocates every time we send a packet, which isn't great
        let mut buf = std::io::MemWriter::new();

        header.write_to(&mut buf).unwrap();
        buf.write(msg).unwrap();

        let datagram = buf.unwrap();
        self.outgoing_datagrams.send(datagram);

        self.inflight.push(PacketInfo {
            header: header,
            sent_time: ::time::precise_time_s()
        });

        self.outgoing_seq += 1;

        header.seq
    }
    
    fn parse_datagram<'a>(&mut self, datagram: &'a [u8]) -> IoResult<(PacketHeader, Vec<u8>)> {
        let mut buf = std::io::BufReader::new(datagram);

        let header = PacketHeader::read_from(&mut buf);

        let payload = buf.read_to_end();

        Ok((try!(header), try!(payload)))
    }

    fn is_header_valid(&self, header: &PacketHeader) -> bool {
        use self::sequence::overflow_aware_compare;

        if overflow_aware_compare(header.seq, self.incoming_seq) == Greater {
            overflow_aware_compare(header.ack, self.outgoing_seq) != Greater // can't ack in the future
        } else {
            false
        }
    }

    fn ack(&mut self, header: &PacketHeader) {
        use self::sequence::overflow_aware_compare;

        self.incoming_seq = header.seq;
        self.outgoing_seq_acked = header.ack;

        let curtime = ::time::precise_time_s();
        let mut dropped = 0u64;

        while !self.inflight.is_empty() {
            let oldest = self.inflight.front().map(|pkt| pkt.header.seq).unwrap();

            if overflow_aware_compare(oldest, header.seq) == Greater {
                break
            } else {
                let PacketInfo { sent_time, ..} = self.inflight.pop_front().unwrap();

                self.latency = curtime - sent_time;
                dropped += 1;
            }
        }

        self.dropped_packets = dropped;
    }
    pub fn recv(&mut self) -> Result<Vec<Vec<u8>>, RecvError> {
        let mut messages = Vec::new();
        loop {
            match self.incoming_datagrams.try_recv() {
                Ok(datagram) => {
                    match self.parse_datagram(datagram.as_slice()) {
                        Ok((header, payload)) => {
                            if self.is_header_valid(&header) {
                                self.ack(&header);

                                messages.push(payload);
                            } else {
                                // bad packet header
                            }
                        },
                        Err(_) => {
                            // corrupt packet or something
                        }
                    }
                },
                Err(std::comm::Empty) => {
                    break;
                },
                Err(std::comm::Disconnected) => {
                    return Err(TaskDied);
                }
            }
        }

        Ok(messages)
    }

    fn create_header(&self) -> PacketHeader {
        PacketHeader {
            seq: self.outgoing_seq,
            ack: self.incoming_seq
        }
    }

    pub fn is_acknowledged(&self, seq: SequenceNr) -> bool {
        use self::sequence::overflow_aware_compare;

        overflow_aware_compare(self.outgoing_seq_acked, seq) != Greater
    }
}

#[cfg(test)]
mod test {
    use super::NetChannel;

    fn build_netchannel() -> (NetChannel, Sender<Vec<u8>>, Receiver<Vec<u8>>) {
        let (outgoing_tx, outgoing_rx) = channel();
        let (incoming_tx, incoming_rx) = channel();

        (
            NetChannel::new_from_channels(incoming_rx, outgoing_tx),
            incoming_tx,
            outgoing_rx
        )
    }

    #[test]
    fn smoke_netchannel() {
        let (mut chan1, packets_tx1, packets_rx1) = build_netchannel();
        let (mut chan2, packets_tx2, packets_rx2) = build_netchannel();


        let seq = chan1.transmit(b"Candygram!");
        packets_tx2.send(packets_rx1.try_recv().unwrap());

        assert_eq!(chan2.recv().unwrap()[0].as_slice(), b"Candygram!");
        
        chan2.transmit(b"Candygram!");
        packets_tx1.send(packets_rx2.try_recv().unwrap());

        assert_eq!(chan1.recv().unwrap()[0].as_slice(), b"Candygram!");

        assert!(chan1.is_acknowledged(seq));
    }
}
