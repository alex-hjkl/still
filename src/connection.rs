use crate::api::Navidrome;

#[derive(Default, PartialEq, Eq)]
pub enum Activity {
    #[default]
    Idle,
    Restoring,
    Connecting,
    Saving,
    Forgetting,
}

pub struct ConnectionState {
    pub client: Option<Navidrome>,
    pub remember: bool,
    pub saved: bool,
    pub revision: u64,
    pub session_id: u64,
    pub activity: Activity,
}

impl ConnectionState {
    pub fn new(from_environment: bool) -> Self {
        Self {
            client: None,
            remember: !from_environment,
            saved: false,
            revision: 0,
            session_id: 0,
            activity: Activity::Idle,
        }
    }

    pub fn is_busy(&self) -> bool {
        self.activity != Activity::Idle
    }

    pub fn disconnect(&mut self) {
        self.client = None;
        self.session_id = self.session_id.wrapping_add(1);
        self.activity = Activity::Idle;
    }
}
