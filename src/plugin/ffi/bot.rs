use crate::Bot;
use crate::plugin::cast_ref;

pub extern fn bot_get_id(bot: *const ()) -> i64 {
    let bot: &Bot = cast_ref(bot);
    bot.id()
}