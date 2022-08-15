use crate::plugin::cast_ref;
use crate::Bot;

pub extern "C" fn bot_get_id(bot: *const ()) -> i64 {
    let bot: &Bot = cast_ref(bot);
    bot.id()
}
