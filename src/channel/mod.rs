use std;
use std::collections::{Deque, RingBuf};
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

    fn read_from<R: Reader>(&self, r: &mut R) -> IoResult<PacketHeader> {
        Ok(PacketHeader {
            seq: try!(r.read_u8()),
            ack: try!(r.read_u8())
        })
    }
}

pub struct NetChannel {
    incoming_datagrams: Receiver<IoResult<Vec<u8>>>,
    outgoing_datagrams: Sender<Vec<u8>>,

    incoming_seq: SequenceNr,
    outgoing_seq: SequenceNr,
    outgoing_seq_acked: SequenceNr,

    inflight: RingBuf<PacketHeader>,
    reliable_data: Option<Vec<u8>>
}

pub enum RecvError {
    IoFailed(std::io::IoError),
    TaskDied
}

impl NetChannel {
    pub fn new_from_channels(incoming_datagrams: Receiver<IoResult<Vec<u8>>>,
                             outgoing_datagrams: Sender<Vec<u8>>) -> NetChannel {
        NetChannel {
            incoming_datagrams: incoming_datagrams,
            outgoing_datagrams: outgoing_datagrams,

            incoming_seq: 0,
            outgoing_seq: 0,
            outgoing_seq_acked: 0,

            inflight: RingBuf::new(),
            reliable_data: None,
        }
    }

    // Err if you try to send a reliable message while there is already one
    // outstanding
    pub fn transmit(&mut self, msg: &[u8], reliable: bool) -> Result<SequenceNr, &'static str> {
        let result = {
            let msg = match self.reliable_data {
                Some(_) if reliable => {
                    return Err("Trying to send a reliable message, but there is one unacknowledged!");
                },
                Some(ref reliable_msg) => {
                    // reliable message makes us silently drop the unreliable
                    // message we're "supposed" to transmit
                    reliable_msg.as_slice()
                },
                None => msg // nothing reliable, send the unreliable stuff
            };

            let header = self.create_header();

            // TODO: This allocates every time we send a packet, which isn't great
            let mut buf = std::io::MemWriter::new();

            header.write_to(&mut buf).unwrap();
            buf.write(msg).unwrap();

            let datagram = buf.unwrap();
            self.outgoing_datagrams.send(datagram);

            Ok(header.seq)
        };

        if result.is_ok() {
            self.outgoing_seq += 1;
        }

        result
    }
    
    fn parse_datagram<'a>(&mut self, datagram: &'a [u8]) -> Result<&'a [u8], ()> {
        let mut buf = std::io::BufReader::new(datagram);

        
        unimplemented!()
    }

    pub fn recv(&mut self) -> Result<Vec<Vec<u8>>, RecvError> {
        let mut messages = Vec::new();
        loop {
            match self.incoming_datagrams.try_recv() {
                Ok(Ok(datagram)) => {
                    self.parse_datagram(datagram.as_slice()).map(|msg| messages.push(msg.to_vec()));
                },
                Ok(Err(e)) => {
                    return Err(IoFailed(e));
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
}
