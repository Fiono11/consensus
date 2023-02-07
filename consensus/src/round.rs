use std::collections::BTreeSet;
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::thread::sleep;
use std::time::Duration;
use log::debug;
use crate::constants::ROUND_TIMER;
use crate::round::Timer::Active;
use crate::vote::Vote;

#[derive(Debug, Clone, PartialEq)]
pub enum Timer {
    Active,
    Expired,
}

pub type Round = u8;

#[derive(Debug, Clone)]
pub struct RoundState {
    pub(crate) votes: BTreeSet<Vote>,
    pub(crate) voted: bool,
    pub(crate) final_vote: Option<Vote>,
    pub(crate) timer: Arc<(Mutex<Timer>, Condvar)>,
}

impl RoundState {
    pub(crate) fn new(round: Round) -> RoundState {
        let a = Arc::new((Mutex::new(Active), Condvar::new())).clone();
        let timer = Arc::clone(&a);
        thread::spawn(move || {
            sleep(Duration::from_millis(ROUND_TIMER as u64));
            debug!("round {} expired!", round);
            let &(ref mutex, ref cvar) = &*timer;
            let mut value = mutex.lock().unwrap();
            *value = Timer::Expired;
            cvar.notify_one();
        });

        RoundState {
            votes: BTreeSet::new(),
            voted: false,
            timer: a,
            final_vote: None,
        }
    }
}