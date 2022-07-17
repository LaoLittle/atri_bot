# AtriQQ: 亚托莉x螃蟹！
QQ群怎能少得了高性能亚托莉的身影呢？

本项目致力于快速部署，简单使用。

## 注意
本项目仅供学习参考，请勿用于非法或商业用途。

## 特性
- 使用Rust及[ricq](https://github.com/lz1998/ricq)构建
> Rust: 一门赋予每个人的构建可靠且高效软件能力的语言。
> 
> ricq: 基于rust编写的qq协议

## 功能
 - DrawMeme: 奇怪的图片生成器 (移植于[DrawMeme](https://github.com/LaoLittle/DrawMeme))

## 部署
使用登陆帮助程序[rq_login](https://github.com/LaoLittle/rq_login)登陆后得到device和token，
放入bots文件夹内，然后配置登陆信息(位于`service/login.toml`)即可

## TODO
 - [ ] 完善框架
 - [ ] 支持插件化拓展