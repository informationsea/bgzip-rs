use std::sync::mpsc::{Receiver, RecvError, RecvTimeoutError, TryRecvError};

const TIMEOUT_DURATION: std::time::Duration = std::time::Duration::from_millis(10);

pub(crate) fn receive_or_yield<R>(receiver: &Receiver<R>) -> std::result::Result<R, RecvError> {
    loop {
        match receiver.try_recv() {
            Ok(t) => return Ok(t),
            Err(TryRecvError::Empty) => match rayon::yield_now() {
                None => return receiver.recv(),
                Some(rayon::Yield::Executed) => continue,
                Some(rayon::Yield::Idle) => match receiver.recv_timeout(TIMEOUT_DURATION) {
                    Ok(t) => return Ok(t),
                    Err(RecvTimeoutError::Timeout) => {
                        //dbg!("receive idle");
                        continue;
                    }
                    Err(RecvTimeoutError::Disconnected) => return Err(RecvError),
                },
            },
            Err(TryRecvError::Disconnected) => return Err(RecvError),
        }
    }
}
