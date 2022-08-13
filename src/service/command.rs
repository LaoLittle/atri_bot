use crate::service::ServiceHandler;
use dashmap::DashMap;

pub struct CommandService {
    commands: DashMap<String, Box<dyn ServiceHandler>>,
}
