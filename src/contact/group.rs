use std::fmt::{Display, Formatter};
use std::sync::Arc;
use ricq::structs::GroupInfo;
use crate::Bot;

#[derive(Debug, Clone)]
pub struct Group(Arc<imp::Group>);

impl Group {
    pub fn from(bot: Bot, info: GroupInfo) -> Self {
        let imp = imp::Group {
            id: info.code,
            bot,
        };

        Self(Arc::new(imp))
    }

    pub fn id(&self) -> i64 {
        self.0.id
    }
    
    pub fn bot(&self) -> Bot {
        self.0.bot.clone()
    }
}

impl Display for Group {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Group({})", self.id())
    }
}

mod imp {
    use crate::Bot;

    #[derive(Debug, Clone)]
    pub struct Group {
        pub id: i64,
        pub bot: Bot,
    }
}