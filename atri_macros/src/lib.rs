use proc_macro::{Delimiter, Group, Ident, Literal, Punct, Spacing, Span, TokenStream, TokenTree};

/// 标记一个结构体或枚举类型, 将其作为本插件的插件实例
///
/// Marks a struct or an enum as a plugin can be loaded by atri_qq plugin module
///
/// ## Usage
///
///
/// ```rust
/// use atri_plugin::Plugin;
/// #[atri_plugin::plugin]
/// struct MyPlugin {
///   // some field
/// }
///
/// impl Plugin for MyPlugin {
///   /*Some impls here*/
/// }
/// ```
/// 请注意有且仅有一个实现了 `atri_plugin::Plugin` 的结构体或枚举可以被标记为 `plugin`
///
/// Note that only one struct or enum that impls `atri_plugin::Plugin` trait can be
/// mark as a plugin

#[proc_macro_attribute]
pub fn plugin(_attr: TokenStream, input: TokenStream) -> TokenStream {
    let mut tree: Vec<TokenTree> = input.into_iter().collect();

    let name = {
        let mut iter = tree.iter();

        let mut token = None::<&TokenTree>;
        while let Some(t) = iter.next() {
            let str = t.to_string();
            if str == "struct" || str == "enum" {
                token = iter.next();
                break;
            }
        }
        token.unwrap_or_else(|| panic!("Cannot find struct or enum")).clone()
    };

    tree.push(TokenTree::Punct(Punct::new('#', Spacing::Alone)));
    {
        let no_mangle = TokenTree::Ident(Ident::new("no_mangle", Span::call_site()));
        let group = TokenStream::from(no_mangle);
        tree.push(TokenTree::Group(Group::new(Delimiter::Bracket, group)));
    }
    tree.push(TokenTree::Ident(Ident::new("extern", Span::call_site())));
    tree.push(TokenTree::Literal(Literal::string("C")));
    tree.push(TokenTree::Ident(Ident::new("fn", Span::call_site())));
    tree.push(TokenTree::Ident(Ident::new("on_init", Span::call_site())));
    tree.push(TokenTree::Group(Group::new(Delimiter::Parenthesis, TokenStream::new())));
    tree.push(TokenTree::Punct(Punct::new('-', Spacing::Joint)));
    tree.push(TokenTree::Punct(Punct::new('>', Spacing::Alone)));
    tree.push(TokenTree::Ident(Ident::new("atri_plugin", Span::call_site())));
    tree.push(TokenTree::Punct(Punct::new(':', Spacing::Joint)));
    tree.push(TokenTree::Punct(Punct::new(':', Spacing::Alone)));
    tree.push(TokenTree::Ident(Ident::new("PluginInstance", Span::call_site())));
    {
        let mut group = Vec::<TokenTree>::new();
        group.push(TokenTree::Ident(Ident::new("atri_plugin", Span::call_site())));
        group.push(TokenTree::Punct(Punct::new(':', Spacing::Joint)));
        group.push(TokenTree::Punct(Punct::new(':', Spacing::Alone)));
        group.push(TokenTree::Ident(Ident::new("__get_instance", Span::call_site())));

        // param
        {
            let mut new_instance = Vec::<TokenTree>::new();
            new_instance.push(TokenTree::Punct(Punct::new('<', Spacing::Alone)));
            new_instance.push(name);
            new_instance.push(TokenTree::Ident(Ident::new("as", Span::call_site())));
            new_instance.push(TokenTree::Ident(Ident::new("atri_plugin", Span::call_site())));
            new_instance.push(TokenTree::Punct(Punct::new(':', Spacing::Joint)));
            new_instance.push(TokenTree::Punct(Punct::new(':', Spacing::Alone)));
            new_instance.push(TokenTree::Ident(Ident::new("Plugin", Span::call_site())));
            new_instance.push(TokenTree::Punct(Punct::new('>', Spacing::Joint)));
            new_instance.push(TokenTree::Punct(Punct::new(':', Spacing::Joint)));
            new_instance.push(TokenTree::Punct(Punct::new(':', Spacing::Alone)));
            new_instance.push(TokenTree::Ident(Ident::new("new", Span::call_site())));
            new_instance.push(TokenTree::Group(Group::new(Delimiter::Parenthesis, TokenStream::new())));
            group.push(TokenTree::Group(Group::new(Delimiter::Parenthesis, TokenStream::from_iter(new_instance))));
        }
        tree.push(TokenTree::Group(Group::new(Delimiter::Brace, TokenStream::from_iter(group))));
    }

    TokenStream::from_iter(tree)
}