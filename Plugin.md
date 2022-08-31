# 插件的加载方式及接口描述

本文档适用于: 
- 想要理解插件工作方式的开发者
- 希望移植到其他语言以便其他语言可以开发插件的开发者

如果您是开发插件的开发者, 请参阅[插件开发文档](atri_plugin/README.md)


**本文档所提及的`插件`均为可以被AtriQQ直接加载的原生动态库插件,
其余的一切不符合上述要求的`插件`均不在本文范畴内**

## 接口稳定性
插件均使用`C abi`保证插件的ABI层面接口稳定(如果您不知道何为C abi请移步[Bing](https://www.bing.com))

## 加载
所有的`插件`都应暴露两个函数:
`atri_manager_init`和`on_init`

插件位于加载目录下时会先通过系统加载动态库,
然后搜寻上述的两个函数。

### atri_manager_init
本函数为插件管理器的初始化函数,
插件加载会最先调用本函数,
函数接收一个结构体`AtriManager`
(结构体定义位于[ffi.rs](atri_ffi/src/ffi.rs))

函数定义如下
#### Rust:
```rust
#[no_mangle]
unsafe extern "C" fn atri_manager_init(manager: AtriManager) {
    // 在此进行初始化操作
}
```
#### C:
```c
void atri_manager_init(AtriManager manager) {
    // 在此进行初始化操作
}
```

### on_init
本函数为插件实例初始化函数,
调用返回结构体`PluginInstance`作为插件的实例
(结构体定义位于[plugin.rs](atri_ffi/src/plugin.rs))

函数定义如下
#### Rust:
```rust
#[no_mangle]
extern "C" fn on_init() -> PluginInstance {
    // 初始化插件实例
}
```
#### C:
```c
PluginInstance on_init() {
    // 初始化插件实例
}
```

此函数调用后, 插件加载基本完毕

## 启用
在上述加载过程执行完毕返回的插件实例内包含了插件的虚表,
插件启用前会调用插件的`new`函数指针得到插件实例指针
若`should_drop`为`true`,
则每次启用都会通过`new`构造一个实例
(插件实例结构在每次调用`new`时都不应变更)

然后会使用该指针调用`enable`函数用于启用插件

注意: 重复启用一个插件是无效果的

## 禁用
禁用插件会调用`disable`函数,
若`should_drop`为`true`,
则会在`disable`执行完毕后调用`drop`函数销毁插件实例

注意: 重复禁用一个插件是无效果的

## 卸载
卸载插件会先禁用此插件,
然后调用他的`drop`函数销毁插件实例,
最后会释放动态库文件完成整个插件的生命周期

## 交互
插件与主程序可以通过加载阶段得到的`AtriManager`进行交互,
内部的函数指针`get_fun`传入一个`uint16`得到另一个函数指针,
所有的函数定义位于[plugin/ffi/mod.rs](src/plugin/ffi/mod.rs)

推荐在加载阶段将所需的全部函数保存为全局变量

传入未定义的`sig`会得到一个调用就会panic的函数

## 其他
插件的`new`和`drop`规则是迎合Rust设计得到的,
在其他语言可以让`new`函数返回固定的实例,
`drop`函数作为一个无效函数传入(不可为null),