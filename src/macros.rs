#[macro_export]
macro_rules! check_group {
    ($e:tt) => {
        {
        use crate::get_app;
        let group_id = $e.inner.group_code;
        let bot_id = $e.client.uin().await;
        
            let group_bot = get_app().group_bot(group_id);

            if let Some(id) = group_bot {
                if id != bot_id { return; }
            } else {
                get_app().set_group_bot(group_id, bot_id);
            }
        }
    };
}

#[macro_export]
macro_rules! unwrap_result_or_print_err_return {
    ($($x:tt)+) => {
        match ($($x)+) {
            Ok(val) => val,
            Err(err) => {
                use tracing::error;
                error!("{:?}", err);
                return;
            },
        }
    };
}