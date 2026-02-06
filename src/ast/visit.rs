//! Visitor trait for AST traversal
//!
//! Inspired by `syn::visit::Visit`. Provides a trait with a method per AST
//! node type and default walk functions that recurse into children.

use super::File;
use super::expr::*;
use super::item::*;
use super::stmt::*;
use super::ty::*;

/// Visitor trait for immutable AST traversal.
///
/// Each `visit_*` method has a default implementation that walks into the
/// node's children. Override individual methods to intercept specific nodes.
pub trait Visit<'de> {
    fn visit_file(&mut self, file: &File<'de>) {
        visit_file(self, file);
    }

    fn visit_item(&mut self, item: &Item<'de>) {
        visit_item(self, item);
    }

    fn visit_item_fn(&mut self, item: &ItemFn<'de>) {
        visit_item_fn(self, item);
    }

    fn visit_item_struct(&mut self, item: &ItemStruct<'de>) {
        visit_item_struct(self, item);
    }

    fn visit_item_class(&mut self, item: &ItemClass<'de>) {
        visit_item_class(self, item);
    }

    fn visit_item_enum(&mut self, item: &ItemEnum<'de>) {
        visit_item_enum(self, item);
    }

    fn visit_item_namespace(&mut self, item: &ItemNamespace<'de>) {
        visit_item_namespace(self, item);
    }

    fn visit_item_template(&mut self, item: &ItemTemplate<'de>) {
        visit_item_template(self, item);
    }

    fn visit_expr(&mut self, expr: &Expr<'de>) {
        visit_expr(self, expr);
    }

    fn visit_stmt(&mut self, stmt: &Stmt<'de>) {
        visit_stmt(self, stmt);
    }

    fn visit_type(&mut self, ty: &Type<'de>) {
        visit_type(self, ty);
    }

    fn visit_block(&mut self, block: &Block<'de>) {
        visit_block(self, block);
    }

    fn visit_ident(&mut self, _ident: &Ident<'de>) {}

    fn visit_path(&mut self, path: &Path<'de>) {
        visit_path(self, path);
    }

    fn visit_signature(&mut self, sig: &Signature<'de>) {
        visit_signature(self, sig);
    }

    fn visit_member(&mut self, member: &Member<'de>) {
        visit_member(self, member);
    }
}

// --- Default walk functions ---

pub fn visit_file<'de, V: Visit<'de> + ?Sized>(v: &mut V, file: &File<'de>) {
    for item in &file.items {
        v.visit_item(item);
    }
}

pub fn visit_item<'de, V: Visit<'de> + ?Sized>(v: &mut V, item: &Item<'de>) {
    match item {
        Item::Fn(i) => v.visit_item_fn(i),
        Item::Struct(i) => v.visit_item_struct(i),
        Item::Class(i) => v.visit_item_class(i),
        Item::Enum(i) => v.visit_item_enum(i),
        Item::Union(_) => {}
        Item::Namespace(i) => v.visit_item_namespace(i),
        Item::Use(_) => {}
        Item::Type(i) => v.visit_type(&i.ty),
        Item::Typedef(i) => v.visit_type(&i.ty),
        Item::Const(i) => {
            v.visit_type(&i.ty);
            v.visit_expr(&i.expr);
        }
        Item::Static(i) => {
            v.visit_type(&i.ty);
            if let Some(expr) = &i.expr {
                v.visit_expr(expr);
            }
        }
        Item::ForeignMod(i) => {
            for fi in &i.items {
                match fi {
                    ForeignItem::Fn(f) => v.visit_item_fn(f),
                    ForeignItem::Static(_) | ForeignItem::Verbatim(_) => {}
                }
            }
        }
        Item::Template(i) => v.visit_item_template(i),
        Item::StaticAssert(i) => {
            v.visit_expr(&i.expr);
        }
        Item::Macro(_) | Item::Verbatim(_) => {}
    }
}

pub fn visit_item_fn<'de, V: Visit<'de> + ?Sized>(v: &mut V, item: &ItemFn<'de>) {
    v.visit_signature(&item.sig);
    if let Some(block) = &item.block {
        v.visit_block(block);
    }
}

pub fn visit_item_struct<'de, V: Visit<'de> + ?Sized>(v: &mut V, item: &ItemStruct<'de>) {
    if let Some(ident) = &item.ident {
        v.visit_ident(ident);
    }
    if let Fields::Named(fields) = &item.fields {
        for member in &fields.members {
            v.visit_member(member);
        }
    }
}

pub fn visit_item_class<'de, V: Visit<'de> + ?Sized>(v: &mut V, item: &ItemClass<'de>) {
    if let Some(ident) = &item.ident {
        v.visit_ident(ident);
    }
    if let Fields::Named(fields) = &item.fields {
        for member in &fields.members {
            v.visit_member(member);
        }
    }
}

pub fn visit_item_enum<'de, V: Visit<'de> + ?Sized>(v: &mut V, item: &ItemEnum<'de>) {
    if let Some(ident) = &item.ident {
        v.visit_ident(ident);
    }
    for variant in item.variants.iter() {
        v.visit_ident(&variant.ident);
        if let Some(disc) = &variant.discriminant {
            v.visit_expr(disc);
        }
    }
}

pub fn visit_item_namespace<'de, V: Visit<'de> + ?Sized>(v: &mut V, item: &ItemNamespace<'de>) {
    for inner in &item.content {
        v.visit_item(inner);
    }
}

pub fn visit_item_template<'de, V: Visit<'de> + ?Sized>(v: &mut V, item: &ItemTemplate<'de>) {
    v.visit_item(&item.item);
}

pub fn visit_member<'de, V: Visit<'de> + ?Sized>(v: &mut V, member: &Member<'de>) {
    match member {
        Member::Field(f) => {
            v.visit_type(&f.ty);
            if let Some(val) = &f.default_value {
                v.visit_expr(val);
            }
        }
        Member::Method(f) => v.visit_item_fn(f),
        Member::Constructor(c) => {
            if let Some(block) = &c.block {
                v.visit_block(block);
            }
        }
        Member::Destructor(d) => {
            if let Some(block) = &d.block {
                v.visit_block(block);
            }
        }
        Member::Item(item) => v.visit_item(item),
        Member::Friend(f) => v.visit_item(&f.item),
        Member::AccessSpecifier(_) | Member::Using(_) | Member::StaticAssert(_) => {}
    }
}

pub fn visit_signature<'de, V: Visit<'de> + ?Sized>(v: &mut V, sig: &Signature<'de>) {
    v.visit_type(&sig.return_type);
    v.visit_ident(&sig.ident);
    for arg in sig.inputs.iter() {
        v.visit_type(&arg.ty);
        if let Some(ident) = &arg.ident {
            v.visit_ident(ident);
        }
    }
}

pub fn visit_block<'de, V: Visit<'de> + ?Sized>(v: &mut V, block: &Block<'de>) {
    for stmt in &block.stmts {
        v.visit_stmt(stmt);
    }
}

pub fn visit_stmt<'de, V: Visit<'de> + ?Sized>(v: &mut V, stmt: &Stmt<'de>) {
    match stmt {
        Stmt::Local(local) => {
            v.visit_type(&local.ty);
            v.visit_ident(&local.ident);
            if let Some(init) = &local.init {
                v.visit_expr(init);
            }
        }
        Stmt::Item(item) => v.visit_item(item),
        Stmt::Expr(se) => v.visit_expr(&se.expr),
        Stmt::Return(ret) => {
            if let Some(expr) = &ret.expr {
                v.visit_expr(expr);
            }
        }
        Stmt::If(i) => {
            v.visit_expr(&i.condition);
            v.visit_stmt(&i.then_body);
            if let Some(else_body) = &i.else_body {
                v.visit_stmt(else_body);
            }
        }
        Stmt::While(w) => {
            v.visit_expr(&w.condition);
            v.visit_stmt(&w.body);
        }
        Stmt::DoWhile(dw) => {
            v.visit_stmt(&dw.body);
            v.visit_expr(&dw.condition);
        }
        Stmt::For(f) => {
            if let Some(init) = &f.init {
                v.visit_stmt(init);
            }
            if let Some(cond) = &f.condition {
                v.visit_expr(cond);
            }
            if let Some(incr) = &f.increment {
                v.visit_expr(incr);
            }
            v.visit_stmt(&f.body);
        }
        Stmt::ForRange(fr) => {
            v.visit_type(&fr.ty);
            v.visit_expr(&fr.range);
            v.visit_stmt(&fr.body);
        }
        Stmt::Switch(sw) => {
            v.visit_expr(&sw.expr);
            v.visit_block(&sw.body);
        }
        Stmt::Case(c) => v.visit_expr(&c.value),
        Stmt::TryCatch(tc) => {
            v.visit_block(&tc.try_body);
            for catch in &tc.catches {
                v.visit_block(&catch.body);
            }
        }
        Stmt::Block(b) => v.visit_block(b),
        Stmt::Break(_)
        | Stmt::Continue(_)
        | Stmt::Goto(_)
        | Stmt::Label(_)
        | Stmt::Default(_)
        | Stmt::Empty => {}
    }
}

pub fn visit_expr<'de, V: Visit<'de> + ?Sized>(v: &mut V, expr: &Expr<'de>) {
    match expr {
        Expr::Lit(_) | Expr::Bool(_) | Expr::Nullptr(_) | Expr::This(_) => {}
        Expr::Ident(e) => v.visit_ident(&e.ident),
        Expr::Path(e) => v.visit_path(&e.path),
        Expr::Paren(e) => v.visit_expr(&e.expr),
        Expr::Unary(e) => v.visit_expr(&e.operand),
        Expr::Binary(e) => {
            v.visit_expr(&e.lhs);
            v.visit_expr(&e.rhs);
        }
        Expr::Conditional(e) => {
            v.visit_expr(&e.condition);
            v.visit_expr(&e.then_expr);
            v.visit_expr(&e.else_expr);
        }
        Expr::Call(e) => {
            v.visit_expr(&e.func);
            for arg in e.args.iter() {
                v.visit_expr(arg);
            }
        }
        Expr::MethodCall(e) => {
            v.visit_expr(&e.receiver);
            for arg in e.args.iter() {
                v.visit_expr(arg);
            }
        }
        Expr::Index(e) => {
            v.visit_expr(&e.object);
            v.visit_expr(&e.index);
        }
        Expr::Field(e) => v.visit_expr(&e.object),
        Expr::Cast(e) => {
            v.visit_type(&e.ty);
            v.visit_expr(&e.expr);
        }
        Expr::CStyleCast(e) => {
            v.visit_type(&e.ty);
            v.visit_expr(&e.expr);
        }
        Expr::Sizeof(e) => v.visit_expr(&e.operand),
        Expr::Alignof(e) => v.visit_type(&e.ty),
        Expr::New(e) => v.visit_type(&e.ty),
        Expr::Delete(e) => v.visit_expr(&e.expr),
        Expr::Throw(e) => {
            if let Some(expr) = &e.expr {
                v.visit_expr(expr);
            }
        }
        Expr::Lambda(e) => v.visit_block(&e.body),
        Expr::Typeid(_) => {}
        Expr::InitList(e) => {
            for elem in &e.elements {
                v.visit_expr(elem);
            }
        }
    }
}

pub fn visit_type<'de, V: Visit<'de> + ?Sized>(v: &mut V, ty: &Type<'de>) {
    match ty {
        Type::Fundamental(_) | Type::Auto(_) => {}
        Type::Path(t) => v.visit_path(&t.path),
        Type::Ptr(t) => v.visit_type(&t.pointee),
        Type::Reference(t) => v.visit_type(&t.referent),
        Type::RvalueReference(t) => v.visit_type(&t.referent),
        Type::Array(t) => {
            v.visit_type(&t.element);
            if let Some(size) = &t.size {
                v.visit_expr(size);
            }
        }
        Type::FnPtr(t) => {
            v.visit_type(&t.return_type);
            for param in t.params.iter() {
                v.visit_type(param);
            }
        }
        Type::Decltype(t) => v.visit_expr(&t.expr),
        Type::TemplateInst(t) => {
            v.visit_path(&t.path);
            for arg in &t.args {
                match arg {
                    TemplateArg::Type(ty) => v.visit_type(ty),
                    TemplateArg::Expr(expr) => v.visit_expr(expr),
                }
            }
        }
        Type::Qualified(t) => v.visit_type(&t.ty),
    }
}

pub fn visit_path<'de, V: Visit<'de> + ?Sized>(v: &mut V, path: &Path<'de>) {
    for seg in &path.segments {
        v.visit_ident(&seg.ident);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::parse_file;

    #[test]
    fn visitor_counts_functions() {
        struct FnCounter {
            count: usize,
        }
        impl<'de> Visit<'de> for FnCounter {
            fn visit_item_fn(&mut self, _: &ItemFn<'de>) {
                self.count += 1;
            }
        }

        let file = parse_file("int foo(); void bar() { }").unwrap();
        let mut counter = FnCounter { count: 0 };
        counter.visit_file(&file);
        assert_eq!(counter.count, 2);
    }

    #[test]
    fn visitor_counts_namespaces() {
        struct NsCounter {
            count: usize,
        }
        impl<'de> Visit<'de> for NsCounter {
            fn visit_item_namespace(&mut self, item: &ItemNamespace<'de>) {
                self.count += 1;
                // Must call default to recurse into nested namespaces
                visit_item_namespace(self, item);
            }
        }

        let file = parse_file("namespace a { namespace b { } } namespace c { }").unwrap();
        let mut counter = NsCounter { count: 0 };
        counter.visit_file(&file);
        assert_eq!(counter.count, 3);
    }

    #[test]
    fn visitor_collects_idents() {
        struct IdentCollector<'a> {
            idents: Vec<&'a str>,
        }
        impl<'de> Visit<'de> for IdentCollector<'de> {
            fn visit_ident(&mut self, ident: &Ident<'de>) {
                self.idents.push(ident.sym);
            }
        }

        let file = parse_file("int add(int a, int b);").unwrap();
        let mut collector = IdentCollector { idents: Vec::new() };
        collector.visit_file(&file);
        assert!(collector.idents.contains(&"add"));
        assert!(collector.idents.contains(&"a"));
        assert!(collector.idents.contains(&"b"));
    }
}
