# 配置你的项目

## Cargo.toml
```toml
[lib]
crate-type = ["cdylib"] # or dylib

[dependencies]
atri_plugin = "0"
```

## 定义插件结构
首先, 定义一个结构体(struct)或枚举(enum)作为插件的结构
```rust
#[atri_plugin::plugin] // 使用此宏标记其为插件
struct MyPlugin {
    listener: Option<ListenerGuard>,
}
```

## 为插件结构实现`Plugin`
```rust
use atri_plugin::Plugin;
impl Plugin for MyPlugin {
    fn new() -> Self {
        Self { listener: None }
    }
    
    fn enable(&mut self) {
        info!("Enable my plugin");

        let guard = Listener::listening_on_always(|e: GroupMessageEvent| async move {
            let message = e.message();
            if message.to_string() == "123" {
                let mut chain = MessageChainBuilder::new();
                chain.push_str("321")
                    .push_str("114514");
                let _ = e.group().send_message(chain.build()).await;
            }
        });
        self.listener = Some(guard);
    }

    fn disable(&mut self) {
        info!("Disable my plugin");
    }
}
```