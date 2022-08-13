use dashmap::DashMap;
use crate::service::ServiceHandler;

pub struct CommandService {
    commands: DashMap<String, Box<dyn ServiceHandler>>,
}