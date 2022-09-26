# AtriPlugin

[![crates.io](https://img.shields.io/crates/v/atri-plugin?label=latest)](https://crates.io/crates/atri-plugin)

编写可以被AtriBot所加载的插件

### 开发示例
Cargo.toml: 
```toml
[lib]
crate-type = ["cdylib"] # or dylib

[dependencies]
atri_plugin = "0"
```

首先, 定义一个结构体(struct)或枚举(enum)作为插件的实例
```rust
#[atri_plugin::plugin] // 使用此宏标记其为插件
struct MyPlugin {
    listener: Option<ListenerGuard>,
}
```

为其实现`Plugin`
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

    fn should_drop() -> bool {
        true
    }
}
```

也可以为插件实现`Drop`, 将根据规则释放

最后, 将`cargo build`编译得到的动态库放入`AtriBot`的`plugins`文件夹内,
开启`AtriBot`即可

详细文档另请参阅本crate源码

### 插件依赖
所有的插件依赖都应被放入`plugins/dependencies`文件夹内,
在加载插件动态库前会先加载此文件夹内所有的动态库文件