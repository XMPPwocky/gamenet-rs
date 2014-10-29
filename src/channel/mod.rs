use std;
use std::collections::{Deque, RingBuf};
use std::io::IoResult;

mod sequence;

/*struct Frame<T: NetMessage> {
    seq: sequence::SequenceNr,
    ack: sequence::SequenceNr,
}*/

pub trait NetMessage {
    fn write_to<W: Writer>(&self, w: &mut W) -> IoResult<()>;
}

pub struct NetChannel<T: NetMessage> {
    incoming_datagrams: Receiver<IoResult<Vec<u8>>>,
    outgoing_datagrams: Sender<Vec<u8>>,

    incoming_seq: sequence::SequenceNr,
    outgoing_seq: sequence::SequenceNr,
    outgoing_seq_acked: sequence::SequenceNr,

    queued_unreliables: RingBuf<T>,
    queued_reliables: RingBuf<T>,

   // inflight_frames: RingBuf<Frame<T>>,
    reliable_data: Option<Vec<T>>
}

pub enum RecvError {
    IoFailed(std::io::IoError),
    TaskDied
}

impl<T: NetMessage> NetChannel<T> {
    pub fn new_from_channels(incoming_datagrams: Receiver<IoResult<Vec<u8>>>,
                             outgoing_datagrams: Sender<Vec<u8>>) -> NetChannel<T> {
        NetChannel {
            incoming_datagrams: incoming_datagrams,
            outgoing_datagrams: outgoing_datagrams,

            incoming_seq: 0,
            outgoing_seq: 0,
            outgoing_seq_acked: 0,

            queued_unreliables: RingBuf::new(),
            queued_reliables: RingBuf::new(),

            //inflight_frames: RingBuf::new(),
            reliable_data: None
        }
    }

    pub fn send_unreliable(&mut self, msg: T) {
        self.queued_unreliables.push(msg);
    }

    pub fn send_reliable(&mut self, msg: T) {
        self.queued_reliables.push(msg);
    }

    pub fn recv(&mut self) -> Result<Vec<T>, RecvError> {
        let mut messages = Vec::new();
        loop {
            match self.incoming_datagrams.try_recv() {
                Ok(Ok(datagram)) => {
                    let _ = datagram;
                    unimplemented!();
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

    fn write_frame_header<W: Writer>(&mut self, _w: &mut W) -> IoResult<()> {
        self.outgoing_seq += 1;
        unimplemented!()
    }

    /// Return value indicates whether there is still data pending.
    pub fn send_frame(&mut self) -> bool {
        let mut buf = Vec::from_elem(1400, 0u8);

        let actual_size = {
            let mut w = std::io::BufWriter::new(buf.as_mut_slice());

            self.write_frame_header(&mut w).unwrap();

            match self.reliable_data {
                Some(ref reliables) => {

                    for reliable in reliables.iter() {
                        let result = reliable.write_to(&mut w);
                        if result.is_err() {
                            break; // out of space.
                        }
                    }
                },
                None => {
                    let mut sent_reliables = Vec::new();
                    loop {
                        match self.queued_reliables.pop_front() {
                            Some(reliable) => {
                                if reliable.write_to(&mut w).is_err() {
                                    break; // out of space
                                }
                                sent_reliables.push(reliable);
                            },
                            None => break
                        }
                    }
                    self.reliable_data = Some(sent_reliables);
                }
            }

            // that's the reliables, now send all the unreliables we can
            loop {
                match self.queued_unreliables.pop_front() {
                    Some(msg) => {
                        if msg.write_to(&mut w).is_err() { break }
                    },
                    None => break
                }
            }

            w.tell().unwrap() as uint
        };

        buf.truncate(actual_size);

        // okay!
        self.outgoing_datagrams.send(buf);

        self.reliable_data.is_some() || !self.queued_reliables.is_empty()
            && !self.queued_unreliables.is_empty()
    }
}
