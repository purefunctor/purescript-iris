use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use super::support::TestWorkspace;

const TIMEOUT: Duration = Duration::from_secs(30);

pub struct SpagoControl {
    address: SocketAddr,
    processes: Receiver<SpagoProcess>,
}

impl SpagoControl {
    pub fn start(workspace: &TestWorkspace) -> SpagoControl {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        workspace.set_env("IRIS_E2E_SPAGO_CONTROL", address.to_string());
        let (sender, processes) = mpsc::channel();
        thread::spawn(move || {
            for control in listener.incoming() {
                let control = control.unwrap();
                let mut role = String::new();
                BufReader::new(&control).read_line(&mut role).unwrap();
                let Some(role) = role.strip_suffix('\n') else {
                    break;
                };
                let process = SpagoProcess { role: role.to_owned(), control };
                if sender.send(process).is_err() {
                    break;
                }
            }
        });
        SpagoControl { address, processes }
    }

    #[track_caller]
    pub fn wait_for_start(&self, role: &str) -> SpagoProcess {
        let process = match self.processes.recv_timeout(TIMEOUT) {
            Ok(process) => process,
            Err(RecvTimeoutError::Timeout) => {
                panic!("timed out waiting for the Spago {role} to start")
            }
            Err(RecvTimeoutError::Disconnected) => panic!("the Spago control listener stopped"),
        };
        assert_eq!(process.role, role, "unexpected Spago process");
        process
    }
}

impl Drop for SpagoControl {
    fn drop(&mut self) {
        let _ = TcpStream::connect(self.address);
    }
}

pub struct SpagoProcess {
    role: String,
    control: TcpStream,
}

impl SpagoProcess {
    pub fn release(&mut self) {
        self.control.write_all(b"release\n").unwrap();
    }

    pub fn is_running(&self) -> bool {
        self.control.set_nonblocking(true).unwrap();
        let running = match self.control.peek(&mut [0]) {
            Ok(0) => false,
            Err(error) if error.kind() == ErrorKind::ConnectionReset => false,
            Err(error) if error.kind() == ErrorKind::WouldBlock => true,
            result => panic!("unexpected control message from the Spago {}: {result:?}", self.role),
        };
        self.control.set_nonblocking(false).unwrap();
        running
    }

    #[track_caller]
    pub fn wait_for_exit(&mut self) {
        self.control.set_read_timeout(Some(TIMEOUT)).unwrap();
        match self.control.read(&mut [0]) {
            Ok(0) => {}
            Err(error) if error.kind() == ErrorKind::ConnectionReset => {}
            Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                panic!("timed out waiting for the Spago {} to exit", self.role)
            }
            result => panic!("unexpected control message from the Spago {}: {result:?}", self.role),
        }
    }
}
