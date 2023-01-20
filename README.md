<img alt="Atri" src="statics/atri.jpg" width="200"/>
----
<div align="center">

QQ群怎能少得了高性能亚托莉的身影呢？

本项目致力于快速部署，简单使用。

项目Logo由[妮娅ko](https://space.bilibili.com/13347846)绘制
</div>

## 声明
本项目仅供学习参考，请勿用于非法或商业用途。

本项目形象均来自《[Atri-MyDearMoments](https://atri-mdm.com)》

## 特性
- 使用Rust及[ricq](https://github.com/lz1998/ricq)构建
> Rust: 一门赋予每个人的构建可靠且高效软件能力的语言。
> 
> ricq: 使用Rust编写的qq协议

- 支持加载原生动态库插件, 高性能低占用

## 部署
使用登陆帮助程序[atri_login](https://github.com/LaoLittle/atri_login)登陆后得到`device`和`token`，
放入`clients`文件夹内，然后配置登陆信息(位于`service/login.toml`)即可

## TODO
 - [ ] 完善事件
 - [ ] 完善消息类型
 - [ ] 完善插件管理

本Bot遵循[AtriPlugin](https://github.com/AtriKawaii/atri_plugin)原生插件加载标准,
若要使用Rust编写插件, AtriPlugin项目提供了友好的接口, 可以快速上手:
[插件开发文档](https://atrikawaii.github.io/atri_doc/)

若需要使用其他的Native语言编写插件, 请参阅:
[插件加载方式](https://github.com/AtriKawaii/atri_plugin/blob/main/Load.md)

## 二次开发
可直接基于本项目进行二次开发, 而不是作为插件加载

配置 Cargo.toml:
```toml
[dependencies]
atri_bot = "0.4.0"
```

### 注意
目前处于开发阶段, 不保证插件接口稳定.
更推荐直接基于本项目进行二次开发

#### *在0.2版本(及以后), 插件提供一定程度的跨版本兼容。