use std::collections::btree_map::{BTreeMap, Iter};
use std::collections::{HashMap, HashSet};
use std::io::{Write, stderr};

use std::result;

use proc_macro2::{Ident, Span, TokenStream};
use quote::{self, ToTokens};

mod xdr_nom;

use xdr::Error;

pub type Result<T> = result::Result<T, Error>;

pub type Comment = String;

pub use self::xdr_nom::specification;

#[cfg(not(feature="derive_strum_enum_string"))]
bitflags! {
    pub struct Derives: u32 {
        const COPY = 1 << 0;
        const CLONE = 1 << 1;
        const DEBUG = 1 << 2;
        const EQ = 1 << 3;
        const PARTIALEQ = 1 << 4;
    }
}

#[cfg(feature="derive_strum_enum_string")]
    bitflags! {
        pub struct Derives: u32 {
            const COPY = 1 << 0;
            const CLONE = 1 << 1;
            const DEBUG = 1 << 2;
            const EQ = 1 << 3;
            const PARTIALEQ = 1 << 4;
            const ENUM_STRING = 1 << 5;
        }
    }

impl ToTokens for Derives {
    fn to_tokens(&self, toks: &mut TokenStream) {
        if self.is_empty() {
            return;
        }

        let mut tokens = toks.to_string();

        #[cfg(feature="reprc")]
        tokens.push_str("#[repr(C)]");

        let mut der = Vec::<&str>::new();

        if self.contains(Derives::COPY) {
            der.push("Copy")
        }
        if self.contains(Derives::CLONE) {
            der.push("Clone")
        }
        if self.contains(Derives::DEBUG) {
            der.push("Debug")
        }
        if self.contains(Derives::EQ) {
            der.push("Eq")
        }
        if self.contains(Derives::PARTIALEQ) {
            der.push("PartialEq")
        }

        #[cfg(feature="derive_strum_enum_string")]
        if self.contains(Derives::ENUM_STRING) {
            der.push("EnumString")
        }

        #[cfg(feature="derive_serde")] {
            der.push("Serialize");
            der.push("Deserialize");
        }

        #[cfg(feature="derive_json_schema")] {
            der.push("JsonSchema");
        }
        
        tokens.push_str(&format!("#[derive({})]", der.join(",")));
        *toks = tokens.parse().unwrap()
    }
}

lazy_static! {
    static ref KEYWORDS: HashSet<&'static str> = {
        let kws = [
            "abstract",	"alignof", "as", "become", "box",
            "break", "const", "continue", "crate", "do",
            "else", "enum", "extern", "false", "final",
            "fn", "for", "if", "impl", "in",
            "let", "loop", "macro", "match", "mod",
            "move", "mut", "offsetof", "override", "priv",
            "proc", "pub", "pure", "ref", "return",
            "Self", "self", "sizeof", "static", "struct",
            "super", "trait", "true", "type", "typeof",
            "unsafe", "unsized", "use", "virtual", "where",
            "while", "yield",
        ];

        kws.iter().map(|x| *x).collect()
    };
}

pub(crate) fn quote_ident<S: AsRef<str>>(id: S) -> Ident {
    let id = id.as_ref();

    if (*KEYWORDS).contains(id) {
        Ident::new(&format!("{}_", id), Span::call_site())
    } else {
        Ident::new(id, Span::call_site())
    }
}

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Clone)]
pub enum Value {
    Ident(String),
    Const(i64),
}

impl Value {
    fn ident<S: AsRef<str>>(id: S) -> Value {
        Value::Ident(id.as_ref().to_string())
    }

    fn as_ident(&self) -> Ident {
        match self {
            &Value::Ident(ref id) => quote_ident(id),
            &Value::Const(val) => {
                Ident::new(&format!(
                    "Const{}{}",
                    (if val < 0 { "_" } else { "" }),
                    val.abs()
                ), Span::call_site())
            }
        }
    }

    fn as_i64(&self, symtab: &Symtab) -> Option<i64> {
        symtab.value(self)
    }

    fn as_token(&self, symtab: &Symtab) -> TokenStream {
        match self {
            &Value::Const(c) => quote!(#c),
            &Value::Ident(ref id) => {
                let tok = quote_ident(id.as_str());
                if let Some((_, Some(ref scope))) = symtab.getconst(id) {
                    let scope = quote_ident(scope);
                    quote!(#scope :: #tok)
                } else {
                    quote!(#tok)
                }
            }
        }
    }
}

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Clone)]
pub enum Type {
    UInt,
    Int,
    UHyper,
    Hyper,
    Float,
    Double,
    Quadruple,
    Bool,

    // Special array elements
    Opaque, // binary
    String, // text

    // Compound types
    Enum(Vec<EnumDefn>),
    Struct(Vec<Decl>),
    Union(Box<Decl>, Vec<UnionCase>, Option<Box<Decl>>),

    Option(Box<Type>),
    Array(Box<Type>, Value),
    Flex(Box<Type>, Option<Value>),

    // Type reference (may be external)
    Ident(String, Option<Derives>),
}

impl Type {
    fn array(ty: Type, sz: Value) -> Type {
        Type::Array(Box::new(ty), sz)
    }

    fn flex(ty: Type, sz: Option<Value>) -> Type {
        Type::Flex(Box::new(ty), sz)
    }

    fn option(ty: Type) -> Type {
        Type::Option(Box::new(ty))
    }

    fn union((d, c, dfl): (Decl, Vec<UnionCase>, Option<Decl>)) -> Type {
        Type::Union(Box::new(d), c, dfl.map(Box::new))
    }

    fn ident<S: AsRef<str>>(id: S) -> Type {
        Type::Ident(id.as_ref().to_string(), None)
    }

    fn ident_with_derives<S: AsRef<str>>(id: S, derives: Derives) -> Type {
        Type::Ident(id.as_ref().to_string(), Some(derives))
    }

    fn is_boxed(&self, symtab: &Symtab) -> bool {
        use self::Type::*;

        match self {
            _ if self.is_prim(symtab) => false,
            &Array(_, _) | &Flex(_, _) | &Option(_) => false,
            &Ident(ref name, _) => {
                if let Some(ty) = symtab.typespec(name) {
                    ty.is_boxed(symtab)
                } else {
                    true
                }
            }
            _ => true,
        }
    }

    fn is_prim(&self, symtab: &Symtab) -> bool {
        use self::Type::*;

        match self {
            &Int | &UInt | &Hyper | &UHyper | &Float | &Double | &Quadruple | &Bool => true,

            &Ident(ref id, _) => {
                match symtab.typespec(id) {
                    None => false,
                    Some(ref ty) => ty.is_prim(symtab),
                }
            }

            _ => false,
        }
    }

    fn derivable(&self, symtab: &Symtab, memo: Option<&mut HashMap<Type, Derives>>) -> Derives {
        use self::Type::*;
        let mut memoset = HashMap::new();

        let memo = match memo {
            None => &mut memoset,
            Some(m) => m,
        };

        if let Some(res) = memo.get(self) {
            return *res;
        }

        // No derives unless we can prove we have some
        memo.insert(self.clone(), Derives::empty());

        #[allow(unused_mut)]
        let mut set = match self {
            &Array(ref ty, ref len) => {
                let ty = ty.as_ref();
                let set = match ty {
                    &Opaque | &String => Derives::EQ | Derives::PARTIALEQ | Derives::COPY | Derives::CLONE | Derives::DEBUG,
                    ref ty => ty.derivable(symtab, Some(memo)),
                };
                match len.as_i64(symtab) {
                    Some(v) if v <= 32 => set,
                    _ => Derives::empty(),   // no #[derive] for arrays > 32
                }
            }
            &Flex(ref ty, ..) => {
                let set = ty.derivable(symtab, Some(memo));
                set & !Derives::COPY // no Copy, everything else OK
            }
            &Enum(_) => {
                #[allow(unused_mut)]
                let mut ders = Derives::EQ | Derives::PARTIALEQ | Derives::COPY | Derives::CLONE | Derives::DEBUG;
                #[cfg(feature="derive_strum_enum_string")]
                    ders.insert(Derives::ENUM_STRING);
                ders
            },
            &Option(ref ty) => ty.derivable(symtab, Some(memo)) & !Derives::COPY,
            &Struct(ref fields) => {
                fields.iter().fold(Derives::all(), |a, f| {
                    a & f.derivable(symtab, memo)
                })
            }

            &Union(_, ref cases, ref defl) => {
                cases.iter().map(|c| &c.1).fold(Derives::all(), |a, c| {
                    a & c.derivable(symtab, memo)
                }) &
                    defl.as_ref().map_or(
                        Derives::all(),
                        |d| d.derivable(symtab, memo),
                    )
            }

            &Ident(_, Some(derives)) => derives,

            &Ident(ref id, None) => {
                match symtab.typespec(id) {
                    None => Derives::empty(),  // unknown, really
                    Some(ref ty) => ty.derivable(symtab, Some(memo)),
                }
            }

            &Float | &Double => Derives::PARTIALEQ | Derives::COPY | Derives::CLONE | Derives::DEBUG,
            ty if ty.is_prim(symtab) => Derives::all(),

            _ => Derives::all() & !Derives::COPY,
        };

        #[cfg(feature="derive_strum_enum_string")]
        if let Enum(_) = self {} else {
            set.remove(Derives::ENUM_STRING);
        }
        memo.insert(self.clone(), set);
        set
    }


    fn packer(&self, val: TokenStream, symtab: &Symtab) -> Result<TokenStream> {
        use self::Type::*;

        let res = match self {
            &Enum(_) => quote!((*#val as i32).pack(out)?),

            &Flex(ref ty, ref maxsz) => {
                let ty = ty.as_ref();
                let maxsz = match maxsz {
                    &None => quote!(None),
                    &Some(ref mx) => {
                        let mx = mx.as_token(symtab);
                        quote!(Some(#mx as usize))
                    }
                };
                match ty {
                    &Opaque => quote!(xdr_codec::pack_opaque_flex(&#val, #maxsz, out)?),
                    &String => quote!(xdr_codec::pack_string(&#val, #maxsz, out)?),
                    _ => quote!(xdr_codec::pack_flex(&#val, #maxsz, out)?),
                }
            }

            &Array(ref ty, _) => {
                let ty = ty.as_ref();
                match ty {
                    &Opaque | &String => {
                        quote!(xdr_codec::pack_opaque_array(&#val[..], #val.len(), out)?)
                    }
                    _ => quote!(xdr_codec::pack_array(&#val[..], #val.len(), out, None)?),
                }
            }

            _ => quote!(#val.pack(out)?),
        };

        trace!("packed {:?} val {:?} => {:?}", self, val, res);
        Ok(res)
    }

    fn is_syn(&self) -> bool {
        use self::Type::*;

        match self {
            &Opaque | &String | &Option(_) | &Ident(..) | &Int | &UInt | &Hyper | &UHyper |
            &Float | &Double | &Quadruple | &Bool => true,
            _ => false,
        }
    }

    fn unpacker(&self, symtab: &Symtab) -> TokenStream {
        use self::Type::*;

        match self {
            &Array(ref ty, ref value) => {
                let ty = ty.as_ref();
                let value = value.as_token(symtab);

                match ty {
                    &Opaque | &String => {
                        quote!({
                            let mut buf: [u8; #value as usize] = [0; #value as usize];
                            let sz = xdr_codec::unpack_opaque_array(input, &mut buf[..], #value as usize)?;
                            (buf, sz)
                        })
                    }
                    ty => {
                        let ty = ty.as_token(symtab).unwrap();
                        // Create the return array as uninitialized, since we don't know what to initialize it until
                        // we can deserialize values. We don't even have a guaranteed value we can populate it with, since
                        // the type may not implement Default (and it would be a waste anyway, since they're going to be
                        // replaced).
                        //
                        // However, having an uninitialized array makes for lots of awkward corner cases.
                        // Even in the common case, we can't simply use `unpack_array`, as it will replace each element
                        // by assignment, but that will Drop any existing value - but in this case that will be undefined
                        // as they're uninitialized. So we need to use `unpack_array_with` that allows us to specify a function
                        // which does the initializing assignment. In this case we use `ptr::write` which overwrites memory
                        // without Dropping the current contents.
                        //
                        // With that solved, we also need to deal with the error cases, where the array could be partially
                        // initialized. For this case, `unpack_array_with` also takes a drop function which deinitializes
                        // the partially initialized elements, so the array is left uninitialized in the failure case.
                        // We can then just use `mem::forget` to dispose of the whole thing.
                        //
                        // We also need to catch panics to make sure the buf is forgotten. It may be partially initialized then
                        // it may leak, but that's better than calling Drop on uninitialized elements.
                        quote!({
                            #[inline]
                            fn uninit_ptr_setter<T>(p: &mut ::std::mem::MaybeUninit<T>, v: T) {
                                p.write(v);
                            }
                            #[inline]
                            fn uninit_ptr_dropper<T>(p: &mut ::std::mem::MaybeUninit<T>) {
                                unsafe { p.assume_init_drop(); }
                            }
                            let mut buf: [::std::mem::MaybeUninit<#ty>; #value as usize] = unsafe { ::std::mem::MaybeUninit::uninit().assume_init() };
                            let res = xdr_codec::unpack_array_with(input, &mut buf[..], #value as usize, uninit_ptr_setter, uninit_ptr_dropper, None);

                            match res {
                                Ok(sz) => {
                                    let buf: [#ty; #value as usize] = unsafe { ::std::mem::transmute(buf) };
                                    (buf, sz)
                                }
                                Err(err) => { return Err(err); }
                            }
                        })
                    }
                }
            }

            &Flex(ref ty, ref maxsz) => {
                let ty = ty.as_ref();
                let maxsz = match maxsz {
                    &None => quote!(None),
                    &Some(ref mx) => {
                        let mx = mx.as_token(symtab);
                        quote!(Some(#mx as usize))
                    }
                };

                match ty {
                    &String => quote!(xdr_codec::unpack_string(input, #maxsz)?),
                    &Opaque => quote!(xdr_codec::unpack_opaque_flex(input, #maxsz)?),
                    _ => quote!(xdr_codec::unpack_flex(input, #maxsz)?),
                }
            }

            _ => quote!(xdr_codec::Unpack::unpack(input)?),
        }
    }

    fn as_token(&self, symtab: &Symtab) -> Result<TokenStream> {
        use self::Type::*;

        let ret = match self {
            &Int => quote!(i32),
            &UInt => quote!(u32),
            &Hyper => quote!(i64),
            &UHyper => quote!(u64),
            &Float => quote!(f32),
            &Double => quote!(f64),
            &Quadruple => quote!(f128),
            &Bool => quote!(bool),

            &String => quote!(String),
            &Opaque => quote!(Vec<u8>),

            &Option(ref ty) => {
                let ty = ty.as_ref();
                let tok = ty.as_token(symtab)?;
                if ty.is_boxed(symtab) {
                    quote!(Option<Box<#tok>>)
                } else {
                    quote!(Option<#tok>)
                }
            }

            &Array(ref ty, ref sz) => {
                let ty = ty.as_ref();
                match ty {
                    &String | &Opaque => {
                        let sztok = sz.as_token(symtab);
                        quote!([u8; #sztok as usize])
                    }
                    ref ty => {
                        let tytok = ty.as_token(symtab)?;
                        let sztok = sz.as_token(symtab);
                        quote!([#tytok; #sztok as usize])
                    }
                }
            }

            &Flex(ref ty, _) => {
                let ty = ty.as_ref();
                match ty {
                    &String => quote!(String),
                    &Opaque => quote!(Vec<u8>),
                    ref ty => {
                        let tok = ty.as_token(symtab)?;
                        quote!(Vec<#tok>)
                    }
                }
            }

            &Ident(ref name, _) => {
                let id = quote_ident(name.as_str());
                quote!(#id)
            }

            _ => return Err(format!("can't have unnamed type {:?}", self).into()),
        };
        Ok(ret)
    }
}

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Clone)]
pub struct EnumDefn(pub String, pub Option<Value>, pub Option<Comment>);

impl EnumDefn {
    fn new<S: AsRef<str>>(id: S, val: Option<Value>, comment: Option<&[u8]>) -> EnumDefn {
        EnumDefn(id.as_ref().to_string(), val, into_comment(comment))
    }
}

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Clone)]
pub struct UnionCase(Value, Decl);

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Clone)]
pub enum Decl {
    Void,
    Named(String, Type, Option<Comment>),
}

fn into_comment(comment: Option<&[u8]>) -> Option<Comment> {
    comment.map(|bytes| {
        let str = String::from_utf8_lossy(bytes);
        format!(" {}", str.trim())
    }.to_owned())
}

fn comment_stream(comment: &Option<Comment>) -> TokenStream {
    comment.as_ref().map(|comment| quote!(
        #[doc = #comment]
        
    )).unwrap_or_default()
}

impl Decl {
    fn named<S: AsRef<str>>(id: S, ty: Type) -> Decl {
        Decl::Named(id.as_ref().to_string(), ty, None)
    }

    fn with_comment(mut self, new_comment: Option<&[u8]>) -> Decl {
        match &mut self {
            Decl::Named(_id, _ty, comment) => *comment = into_comment(new_comment),
            _ => {}
        }
        self
    }

    fn name_as_ident(&self) -> Option<(Ident, &Type)> {
        use self::Decl::*;
        match self {
            &Void => None,
            &Named(ref name, ref ty, ..) => Some((quote_ident(name), ty)),
        }
    }

    fn as_token(&self, symtab: &Symtab) -> Result<Option<(Ident, TokenStream, TokenStream)>> {
        use self::Decl::*;
        match self {
            &Void => Ok(None),
            &Named(ref name, ref ty, ref comment) => {
                let nametok = quote_ident(name.as_str());
                let mut tok = ty.as_token(symtab)?;
                if false && ty.is_boxed(symtab) {
                    tok = quote!(Box<#tok>)
                };
                Ok(Some((nametok, tok, comment_stream(comment))))
            }
        }
    }

    fn derivable(&self, symtab: &Symtab, memo: &mut HashMap<Type, Derives>) -> Derives {
        use self::Decl::*;
        match self {
            &Void => Derives::all(),
            &Named(_, ref ty, ..) => ty.derivable(symtab, Some(memo)),
        }
    }
}

// Specification of a named type
#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Clone)]
pub struct Typespec(pub String, pub Type);

// Named synonym for a type
#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Clone)]
pub struct Typesyn(pub String, pub Type);

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Clone)]
pub struct Const(pub String, pub i64);

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Clone)]
pub enum Defn {
    Typespec(String, Type),
    Typesyn(String, Type),
    Const(String, i64),
}

impl Defn {
    fn typespec<S: AsRef<str>>(id: S, ty: Type) -> Defn {
        Defn::Typespec(id.as_ref().to_string(), ty)
    }

    fn typesyn<S: AsRef<str>>(id: S, ty: Type) -> Defn {
        Defn::Typesyn(id.as_ref().to_string(), ty)
    }

    fn constant<S: AsRef<str>>(id: S, v: i64) -> Defn {
        Defn::Const(id.as_ref().to_string(), v)
    }
}

pub trait Emit {
    fn define(&self, symtab: &Symtab) -> Result<TokenStream>;
}

pub trait Emitpack: Emit {
    fn pack(&self, symtab: &Symtab) -> Result<Option<TokenStream>>;
    fn unpack(&self, symtab: &Symtab) -> Result<Option<TokenStream>>;
}

impl Emit for Const {
    fn define(&self, _: &Symtab) -> Result<TokenStream> {
        let name = quote_ident(&self.0);
        let val = &self.1;

        Ok(quote!(pub const #name: i64 = #val;))
    }
}

impl Emit for Typesyn {
    fn define(&self, symtab: &Symtab) -> Result<TokenStream> {
        let ty = &self.1;
        let name = quote_ident(&self.0);
        let tok = ty.as_token(symtab)?;
        Ok(quote!(pub type #name = #tok;))
    }
}

impl Emit for Typespec {
    fn define(&self, symtab: &Symtab) -> Result<TokenStream> {
        use self::Type::*;

        let name = quote_ident(&self.0);
        let ty = &self.1;

        let ret = match ty {
            &Enum(ref edefs) => {
                let defs: Vec<_> = edefs
                    .iter()
                    .filter_map(|&EnumDefn(ref field, _, ref comment)| if let Some((val, Some(_))) =
                        symtab.getconst(field)
                    {
                        Some((quote_ident(field), val as isize, comment_stream(comment)))
                    } else {
                        None
                    })
                    .map(|(field, val, comment)| quote!(#comment #field = #val,))
                    .collect();

                let derive = ty.derivable(symtab, None);
                quote!(#derive pub enum #name { #(#defs)* })
            }

            &Struct(ref decls) => {
                let decls: Vec<_> = decls
                    .iter()
                    .filter_map(|decl| decl.as_token(symtab).transpose())
                    .map(|res| res.map(|(field, ty, comment)| quote!(#comment pub #field: #ty,)))
                    .collect::<Result<Vec<_>>>()?;

                let derive = ty.derivable(symtab, None);
                quote! {
                    #derive
                    pub struct #name { #(#decls)* }
                }
            }

            &Union(ref selector, ref cases, ref defl) => {
                let selector = selector.as_ref();
                use self::Decl::*;
                use self::Value::*;

                let labelfields = false; // true - include label in enum branch

                // return true if case is compatible with the selector
                let compatcase = |case: &Value| {
                    let seltype = match selector {
                        &Void => return false,
                        &Named(_, ref ty, ..) => ty,
                    };

                    match case {
                        &Const(val) if val < 0 => {
                            match seltype {
                                &Int | &Hyper => true,
                                _ => false,
                            }
                        }

                        &Const(_) => {
                            match seltype {
                                &Int | &Hyper | &UInt | &UHyper => true,
                                _ => false,
                            }
                        }

                        &Ident(ref id) => {
                            if *seltype == Bool {
                                id == "TRUE" || id == "FALSE"
                            } else {
                                if let &Type::Ident(ref selname, _) = seltype {
                                    match symtab.getconst(id) {
                                        Some((_, Some(ref scope))) => scope == selname,
                                        _ => false,
                                    }
                                } else {
                                    false
                                }
                            }
                        }
                    }
                };

                let mut cases: Vec<_> = cases
                    .iter()
                    .map(|&UnionCase(ref val, ref decl)| {
                        if !compatcase(val) {
                            return Err(Error::from(
                                format!("incompat selector {:?} case {:?}", selector, val),
                            ));
                        }

                        let label = val.as_ident();

                        match decl {
                            &Void => Ok(quote!(#label,)),
                            &Named(ref name, ref ty, ref comment) => {
                                let mut tok = ty.as_token(symtab)?;
                                if false && ty.is_boxed(symtab) {
                                    tok = quote!(Box<#tok>)
                                };
                                let comment = comment_stream(comment);
                                if labelfields {
                                    let name = quote_ident(name);
                                    Ok(quote!(#comment #label { #name : #tok },))
                                } else {
                                    Ok(quote!(#comment #label(#tok),))
                                }
                            }
                        }
                    })
                    .collect::<Result<Vec<_>>>()?;

                if let &Some(ref def_val) = defl {
                    let def_val = def_val.as_ref();
                    match def_val {
                        &Named(ref name, ref ty, ref comment) => {
                            let mut tok = ty.as_token(symtab)?;
                            if ty.is_boxed(symtab) {
                                tok = quote!(Box<#tok>)
                            };
                            if labelfields {
                                let name = quote_ident(name);
                                cases.push(quote!(#comment default { #name: #tok },
                                ))
                            } else {
                                cases.push(quote!(#comment default(#tok),))
                            }
                        }
                        &Void => cases.push(quote!(default,)),
                    }
                }

                let derive = ty.derivable(symtab, None);
                quote! {
                    #derive
                    pub enum #name { #(#cases)* }
                }
            }

            &Flex(..) | &Array(..) => {
                let tok = ty.as_token(symtab)?;
                let derive = ty.derivable(symtab, None);
                quote! {
                    #derive
                    pub struct #name(pub #tok);
                }
            }

            _ => {
                let tok = ty.as_token(symtab)?;
                quote!(pub type #name = #tok;)
            }
        };
        Ok(ret)
    }
}

impl Emitpack for Typespec {
    fn pack(&self, symtab: &Symtab) -> Result<Option<TokenStream>> {
        use self::Type::*;
        use self::Decl::*;

        let name = quote_ident(&self.0);
        let ty = &self.1;
        let mut directive = quote!();

        let body: TokenStream = match ty {
            &Enum(_) => {
                directive = quote!(#[inline]);
                ty.packer(quote!(self), symtab)?
            }

            &Struct(ref decl) => {
                let decls: Vec<_> = decl.iter()
                    .filter_map(|d| match d {
                        &Void => None,
                        &Named(ref name, ref ty, ..) => Some((quote_ident(name), ty)),
                    })
                    .map(|(field, ty)| {
                        let p = ty.packer(quote!(self.#field), symtab).unwrap();
                        quote!(#p + )
                    })
                    .collect();
                quote!(#(#decls)* 0)
            }

            &Union(_, ref cases, ref defl) => {
                let mut matches: Vec<_> = cases
                    .iter()
                    .filter_map(|&UnionCase(ref val, ref decl)| {
                        let label = val.as_ident();
                        let disc = val.as_token(symtab);

                        let ret = match decl {
                            &Void => quote!(&#name::#label => (#disc as i32).pack(out)?,),
                            &Named(_, ref ty, ..) => {
                                let pack = match ty.packer(quote!(val), symtab) {
                                    Err(_) => return None,
                                    Ok(p) => p,
                                };
                                quote!(&#name::#label(ref val) => (#disc as i32).pack(out)? + #pack,)
                            }
                        };
                        Some(ret)
                    })
                    .collect();

                if let &Some(ref decl) = defl {
                    let decl = decl.as_ref();
                    // Can't cast a value-carrying enum to i32
                    let default = match decl {
                        &Void => {
                            quote! {
                                &#name::default => return Err(xdr_codec::Error::invalidcase(-1)),
                            }
                        }
                        &Named(..) => {
                            quote! {
                                &#name::default(_) => return Err(xdr_codec::Error::invalidcase(-1)),
                            }
                        }
                    };

                    matches.push(default)
                }

                quote!(match self { #(#matches)* })
            }

            // Array and Flex types are wrapped in tuple structs
            &Flex(..) | &Array(..) => ty.packer(quote!(self.0), symtab)?,

            &Ident(_, _) => return Ok(None),

            _ => {
                if ty.is_prim(symtab) {
                    return Ok(None);
                } else {
                    ty.packer(quote!(self), symtab)?
                }
            }
        };

        trace!("body {:?}", body);

        Ok(Some(quote! {
            impl<Out: xdr_codec::Write> xdr_codec::Pack<Out> for #name {
                #directive
                    fn pack(&self, out: &mut Out) -> xdr_codec::Result<usize> {
                        Ok(#body)
                    }
            }
        }))
    }

    fn unpack(&self, symtab: &Symtab) -> Result<Option<TokenStream>> {
        use self::Type::*;
        use self::Decl::*;

        let self_name = quote_ident(&self.0);
        let ty = &self.1;
        let mut directive = quote!();

        let body = match ty {
            &Enum(ref defs) => {
                directive = quote!(#[inline]);
                let matchdefs: Vec<_> = defs.iter()
                    .filter_map(|&EnumDefn(ref name, ..)| {
                        let tok = quote_ident(name);
                        if let Some((ref _val, ref scope)) = symtab.getconst(name) {
                            // let val = *val as i32;
                            if let &Some(ref _scope) = scope {
                                // let scope = quote_ident(scope);
                                // Some(quote!(#val => #scope :: #tok,))
                                Some(quote!(x if x == #self_name :: #tok as i32 => #self_name :: #tok,))
                            } else {
                                // Some(quote!(#val => #tok,))
                                Some(quote!(x if x == #tok as i32 => #tok,))
                            }
                        } else {
                            println!("unknown ident {}", name);
                            None
                        }
                    })
                    .collect();

                quote!({
                    let (e, esz): (i32, _) = xdr_codec::Unpack::unpack(input)?;
                    sz += esz;
                    match e {
                        #(#matchdefs)*
                        e => return Err(xdr_codec::Error::invalid_named_enum(stringify!(#self_name), e))
                    }
                })
            }

            &Struct(ref decls) => {
                let decls: Vec<_> = decls
                    .iter()
                    .filter_map(|decl| decl.name_as_ident())
                    .map(|(field, ty)| {
                        let unpack = ty.unpacker(symtab);
                        quote!(#field: { let (v, fsz) = #unpack; sz += fsz; v },)
                    })
                    .collect();

                quote!(#self_name { #(#decls)* })
            }

            &Union(ref sel, ref cases, ref defl) => {
                let sel = sel.as_ref();
                let mut matches: Vec<_> =
                    cases.iter()
                        .map(|&UnionCase(ref val, ref decl)| {
                            let label = val.as_ident();
                            let disc = match val.as_i64(symtab) {
                                Some(v) => v as i32,
                                None => return Err(Error::from(format!("discriminant value {:?} unknown", val))),
                            };

                            let ret = match decl {
                                //&Void => quote!(#disc => #name::#label,),
                                &Void => quote!(x if x == (#disc as i32) => #self_name::#label,),
                                &Named(_, ref ty, ..) => {
                                    let unpack = ty.unpacker(symtab);
                                    //quote!(#disc => #name::#label({ let (v, fsz) = #unpack; sz += fsz; v }),)
                                    quote!(x if x == (#disc as i32) => #self_name::#label({ let (v, fsz) = #unpack; sz += fsz; v }),)
                                },
                            };
                            Ok(ret)
                        })
                        .collect::<Result<Vec<_>>>()?;

                if let &Some(ref decl) = defl {
                    let decl = decl.as_ref();
                    let defl = match decl {
                        &Void => quote!(_ => #self_name::default),
                        &Named(_, ref ty, ..) => {
                            let unpack = ty.unpacker(symtab);
                            quote!(_ => #self_name::default({
                                let (v, csz) = #unpack;
                                sz += csz;
                                v
                            }))
                        }
                    };

                    matches.push(defl);
                } else {
                    let defl = quote!(v => return Err(xdr_codec::Error::invalid_named_case(stringify!(#self_name), v as i32)));
                    matches.push(defl);
                }

                let selunpack = match sel {
                    &Void => panic!("void switch selector?"),
                    &Named(_, ref ty, ..) => ty.unpacker(symtab),
                };

                quote!(match { let (v, dsz): (i32, _) = #selunpack; sz += dsz; v } { #(#matches)* })
            }

            &Option(_) => ty.unpacker(symtab),

            &Flex(_, _) | &Array(_, _) => {
                let unpk = ty.unpacker(symtab);
                quote!({ let (v, usz) = #unpk; sz = usz; #self_name(v) })
            }

            &Ident(_, _) => return Ok(None),

            _ if ty.is_prim(symtab) => return Ok(None),
            _ => return Err(Error::from(format!("unimplemented ty={:?}", ty))),
        };

        Ok(Some(quote! {
            impl<In: xdr_codec::Read> xdr_codec::Unpack<In> for #self_name {
                #directive
                    fn unpack(input: &mut In) -> xdr_codec::Result<(#self_name, usize)> {
                        #[allow(unused_assignments)]
                        let mut sz = 0;
                        Ok((#body, sz))
                    }
            }
        }))
    }
}

#[derive(Debug, Clone)]
pub struct Symtab {
    consts: BTreeMap<String, (i64, Option<String>)>,
    typespecs: BTreeMap<String, Type>,
    typesyns: BTreeMap<String, Type>,
}

impl Symtab {
    pub fn new(defns: &Vec<Defn>) -> Symtab {
        let mut ret = Symtab {
            consts: BTreeMap::new(),
            typespecs: BTreeMap::new(),
            typesyns: BTreeMap::new(),
        };

        ret.update_consts(&defns);

        ret
    }

    fn update_consts(&mut self, defns: &Vec<Defn>) {
        for defn in defns {
            match defn {
                &Defn::Typespec(ref name, ref ty) => {
                    self.deftype(name, ty);
                    self.update_enum_consts(name, ty);
                }

                &Defn::Const(ref name, val) => self.defconst(name, val, None),

                &Defn::Typesyn(ref name, ref ty) => {
                    self.deftypesyn(name, ty);
                }
            }
        }
    }

    fn update_enum_consts(&mut self, scope: &String, ty: &Type) {
        let mut err = stderr();
        let mut prev = -1;

        if let &Type::Enum(ref edefn) = ty {
            for &EnumDefn(ref name, ref maybeval, ..) in edefn {
                let v = match maybeval {
                    &None => prev + 1,
                    &Some(ref val) => {
                        match self.value(val) {
                            Some(c) => c,
                            None => {
                                let _ = writeln!(&mut err, "Unknown value {:?}", val);
                                continue;
                            }
                        }
                    }
                };

                prev = v;

                // println!("enum {} -> {}", name, v);
                self.defconst(name, v, Some(scope.clone()));
            }
        }
    }

    fn defconst<S: AsRef<str>>(&mut self, name: S, val: i64, scope: Option<String>) {
        self.consts.insert(From::from(name.as_ref()), (val, scope));
    }

    fn deftype<S: AsRef<str>>(&mut self, name: S, ty: &Type) {
        self.typespecs.insert(From::from(name.as_ref()), ty.clone());
    }

    pub fn deftypesyn<S: AsRef<str>>(&mut self, name: S, ty: &Type) {
        self.typesyns.insert(From::from(name.as_ref()), ty.clone());
    }

    pub fn getconst(&self, name: &String) -> Option<(i64, Option<String>)> {
        match self.consts.get(name) {
            None => None,
            Some(c) => Some(c.clone()),
        }
    }

    pub fn value(&self, val: &Value) -> Option<i64> {
        match val {
            &Value::Const(c) => Some(c),
            &Value::Ident(ref id) => self.getconst(id).map(|(v, _)| v),
        }
    }

    pub fn typespec(&self, name: &String) -> Option<&Type> {
        match self.typespecs.get(name) {
            None => {
                match self.typesyns.get(name) {
                    None => None,
                    Some(ty) => Some(ty),
                }
            }
            Some(ty) => Some(ty),
        }
    }

    pub fn constants(&self) -> Iter<String, (i64, Option<String>)> {
        self.consts.iter()
    }

    pub fn typespecs(&self) -> Iter<String, Type> {
        self.typespecs.iter()
    }

    pub fn typesyns(&self) -> Iter<String, Type> {
        self.typesyns.iter()
    }
}


#[cfg(test)]
mod test;
