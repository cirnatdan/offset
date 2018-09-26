use std::{io, cmp, mem};
use std::marker::PhantomData;
use futures::{Async, AsyncSink, Stream, Sink, Poll};
use tokio_io::{AsyncRead, AsyncWrite};


pub struct AsyncReader<M,E> {
    opt_receiver: Option<M>,
    pending_in: Vec<u8>,
    phantom_error: PhantomData<E>,
}

impl<M,E> AsyncReader<M,E> {
    pub fn new(receiver: M) -> Self {
        AsyncReader {
            opt_receiver: Some(receiver),
            pending_in: Vec::new(),
            phantom_error: PhantomData,
        }
    }
}

impl<M,E> io::Read for AsyncReader<M,E> 
where
    M: Stream<Item=Vec<u8>, Error=E>,
{
    fn read(&mut self, mut buf: &mut [u8]) -> io::Result<usize> {
        let mut total_read = 0; // Total amount of bytes read
        loop {
            // pending_in --> buf (As many bytes as possible)
            let min_len = cmp::min(buf.len(), self.pending_in.len());
            buf[.. min_len].copy_from_slice(&self.pending_in[.. min_len]);
            let _ = self.pending_in.drain(.. min_len);
            buf = &mut buf[min_len ..];
            total_read += min_len;

            if buf.is_empty() {
                return Ok(total_read);
            }

            match self.opt_receiver.take() {
                Some(mut receiver) => {
                    match receiver.poll() {
                        Ok(Async::Ready(Some(data))) => {
                            self.opt_receiver = Some(receiver);
                            self.pending_in = data;
                        },
                        Ok(Async::Ready(None)) => return Ok(total_read), // End of incoming data
                        Ok(Async::NotReady) => {
                            self.opt_receiver = Some(receiver);
                            if total_read > 0 {
                                return Ok(total_read)
                            } else {
                                return Err(io::Error::new(io::ErrorKind::WouldBlock, "WouldBlock"))
                            }
                        },
                        Err(_) => return Err(io::Error::new(io::ErrorKind::BrokenPipe, "BrokenPipe")),
                    };
                },
                None => return Ok(total_read),
            }
        }
    }
}


impl<M,E> AsyncRead for AsyncReader<M,E> where M: Stream<Item=Vec<u8>, Error=E> {}


pub struct AsyncWriter<K,E> {
    opt_sender: Option<K>,
    pending_out: Vec<u8>,
    max_frame_len: usize,
    phantom_error: PhantomData<E>,
}


impl<K,E> AsyncWriter<K,E> {
    pub fn new(sender: K, max_frame_len: usize) -> Self {
        AsyncWriter {
            opt_sender: Some(sender),
            pending_out: Vec::new(),
            max_frame_len,
            phantom_error: PhantomData,
        }
    }
}


impl<K,E> io::Write for AsyncWriter<K,E> 
where
    K: Sink<SinkItem=Vec<u8>, SinkError=E>,
{
    fn write(&mut self, mut buf: &[u8]) -> io::Result<usize> {
        let mut sender = match self.opt_sender.take() {
            Some(sender) => sender,
            None => return Err(io::Error::new(io::ErrorKind::BrokenPipe, "BrokenPipe")),
        };
        let mut total_write = 0;

        loop {
            if buf.is_empty() {
                self.opt_sender = Some(sender);
                return Ok(total_write);
            }

            // Buffer as much as possible:
            let free_bytes = self.max_frame_len.checked_sub(self.pending_out.len()).unwrap();
            let min_len = cmp::min(buf.len(), free_bytes);
            self.pending_out.extend_from_slice(&buf[.. min_len]);
            buf = &buf[min_len ..];
            total_write += min_len;

            let pending_out = mem::replace(&mut self.pending_out, Vec::new());
            let is_ready = match sender.start_send(pending_out) {
                Ok(AsyncSink::Ready) => true,
                Ok(AsyncSink::NotReady(pending_out)) => {
                    self.pending_out = pending_out;
                    false
                },
                Err(_) => return Err(io::Error::new(io::ErrorKind::BrokenPipe, "BrokenPipe")),
            };
            if !is_ready {
                self.opt_sender = Some(sender);
                if total_write > 0 {
                    return Ok(total_write);
                } else {
                    return Err(io::Error::new(io::ErrorKind::WouldBlock, "WouldBlock"));
                }
            }
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut sender = match self.opt_sender.take() {
            Some(sender) => sender,
            None => return Err(io::Error::new(io::ErrorKind::BrokenPipe, "BrokenPipe")),
        };

        let is_ready = match sender.poll_complete() {
            Ok(Async::Ready(())) => true,
            Ok(Async::NotReady) => false, 
            Err(_) => return Err(io::Error::new(io::ErrorKind::BrokenPipe, "BrokenPipe")),
        };

        self.opt_sender = Some(sender);
        if !is_ready {
            Err(io::Error::new(io::ErrorKind::WouldBlock, "WouldBlock"))
        } else {
            Ok(())
        }
    }
}

impl<K,E> AsyncWrite for AsyncWriter<K,E> where K: Sink<SinkItem=Vec<u8>, SinkError=E> {
    fn shutdown(&mut self) -> Poll<(), io::Error> {
        match self.opt_sender.take() {
            Some(mut sender) => {
                match sender.close() {
                    Ok(Async::Ready(())) => Ok(Async::Ready(())),
                    Ok(Async::NotReady) => {
                        self.opt_sender = Some(sender);
                        Ok(Async::NotReady)
                    },
                    Err(_) => Err(io::Error::new(io::ErrorKind::BrokenPipe, "BrokenPipe")),
                }
            },
            None => Err(io::Error::new(io::ErrorKind::BrokenPipe, "BrokenPipe")),
        }
    }
}

#[cfg(test)]
#[allow(unused)]
mod tests {
    use super::*;
    use futures::sync::{mpsc, oneshot};
    use futures::prelude::{async, await};
    use futures::Future;
    use tokio_core::reactor::Core;
    use tokio_codec::{FramedRead, FramedWrite};
    use utils::frame_codec::FrameCodec;

    enum ReceiverRes {
        Ready((usize, Vec<u8>)),
        NotReady,
        Error,
    }

    struct TestReceiver {
        receiver: mpsc::Receiver<Vec<u8>>,
        results: Vec<ReceiverRes>,
    }

    /*
    impl Future for TestReceiver {
        fn poll(&mut self) -> Poll<(), Self::Error> {
            let my_buff = [0; 0x100];
            match self.receiver.poll_read(&mut my_buff) {
                Ok(Async::Ready(size)) => self.results.push(
                    (ReceiverRes::Ready(size), my_buff[0..size].to_vec())),
                Ok(Async::NotReady) => self.results.push(ReceiverRes::NotReady),
                Err(_e) => self.results.push(ReceiverRes::Error),
            }
        }
    }
    */

    // TODO: Continue tests here

    #[async]
    fn basic_stream_receiver() -> Result<(), ()> {
        Ok(())
    }

    #[async]
    fn frames_sender(sender: impl Sink<SinkItem=Vec<u8>, SinkError=()> + 'static, 
                     reader_done_send: oneshot::Sender<bool>) 
            -> Result<(), ()> {
        let sender = await!(sender.send(vec![0; 0x200])).unwrap();
        let sender = await!(sender.send(vec![1; 0x100])).unwrap();
        let sender = await!(sender.send(vec![2; 0x80])).unwrap();

        reader_done_send.send(true);
        Ok(())
    }

    #[async]
    fn frames_receiver(receiver: impl Stream<Item=Vec<u8>, Error=()> + 'static, 
                       writer_done_send: oneshot::Sender<bool>) 
            -> Result<(), ()> {
        let (opt_data, receiver) = await!(receiver.into_future()).map_err(|_| ())?;
        writer_done_send.send(true);
        Ok(())
    }

    #[test]
    fn test_basic_stream_receiver() {
        let (sender, receiver) = mpsc::channel::<Vec<u8>>(0);
        let async_reader: AsyncReader<_, _> = AsyncReader::new(receiver);
        let async_writer: AsyncWriter<_, _> = AsyncWriter::new(sender, 0x20);

        let reader = FramedRead::new(async_reader, FrameCodec::new())
            .map_err(|_| ());
        let writer = FramedWrite::new(async_writer, FrameCodec::new())
            .sink_map_err(|_| ());

        let (reader_done_send, reader_done_recv) = oneshot::channel::<bool>();
        let (writer_done_send, writer_done_recv) = oneshot::channel::<bool>();

        // TODO: Continue here.
        // Required: Port async_adapter to use Bytes instead of Vec<u8>

        /*
        let mut core = Core::new().unwrap();
        let handle = core.handle();
        handle.spawn(frames_receiver(reader, reader_done_send));
        handle.spawn(frames_sender(writer, writer_done_send));
        assert_eq!(true, core.run(reader_done_recv).unwrap());
        assert_eq!(true, core.run(writer_done_recv).unwrap());
        */
    }
}
