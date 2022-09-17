![Image](https://socialify.git.ci/LaoLittle/atri_qq/image?descriptionEditable=&font=Inter&forks=1&issues=1&language=1&logo=https%3A%2F%2Fatri-mdm.com%2Fassets%2Fimg%2Fspecial%2Ffaq%2Fthumb02.png&name=1&owner=1&pattern=Plus&pulls=1&stargazers=1&theme=Light)

----
<div style="font-size: 16px; text-align: center">
QQ群怎能少得了高性能亚托莉的身影呢？

本项目致力于快速部署，简单使用。
</div>

## 注意
本项目仅供学习参考，请勿用于非法或商业用途。

本项目形象均来自《[Atri-MyDearMoments](https://atri-mdm.com)》

## 特性
- 使用Rust及[ricq](https://github.com/lz1998/ricq)构建
> Rust: 一门赋予每个人的构建可靠且高效软件能力的语言。
> 
> ricq: 基于rust编写的qq协议

- 支持加载原生动态库插件, 高性能低占用

## 部署
使用登陆帮助程序[atri_login](https://github.com/LaoLittle/atri_login)登陆后得到device和token，
放入bots文件夹内，然后配置登陆信息(位于`service/login.toml`)即可

## TODO
 - [ ] 完善框架
 - [x] 支持插件化拓展

## 进度

- Rust动态库插件
  - [x] 消息链构造
  - [x] 监听消息
    - [x] 群
    - [x] 好友
  - [x] 发送消息
    - [x] 纯文本
    - [x] 图片
    - [x] At/AtAll
  
    ..
若要使用Rust编写插件, 本项目提供了友好的接口, 可以快速上手:
[插件开发文档](https://laolittle.github.io/atri_qq/)

若需要使用其他的Native语言编写插件, 请参阅:
[插件加载方式](Plugin.md)

### 注意
目前处于开发阶段, 不保证插件接口稳定

#### *在0.2版本(及以后), 插件提供一定程度的跨版本兼容。