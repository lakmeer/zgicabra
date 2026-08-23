//
// #[derive(Voice)] -- generates the mechanical skeleton every concrete voice
// in src/audio/*.rs used to hand-write: the read-only *View struct + view(),
// the fields()/apply() snapshot tables, apply_cc(), a UI_RANGES table, an
// optional new(), and the `impl Voice for` block (index, name,
// set_sample_rate, and the tick wrapper that applies thump before calling
// the author's hand-written VoiceDsp::render). Whether this voice is the
// selected one is the caller's call (see audio::engine::Engine) -- the
// generated tick always renders, it doesn't gate on index itself.
// See the proposal in the plan file.
//
// The author writes: the annotated struct (which MUST include a `thump:
// ThumpMod` field and a `sig: SharedSignal` field) + `impl VoiceDsp` (render,
// optional on_block_start/on_silence) + a manual new() when construction needs
// pre-init logic (#[voice(new = manual)]).
//
// Field attributes:
//   #[knob(cc="6|44", range=1.0..10.0, set=|v| 1.0+v*9.0, default=1.0)]
//       a CC-settable, persisted Shared. cc/set optional (a persisted-but-not-
//       CC param omits both). default only needed for a generated new().
//   #[live(range=0.0..5.0)]   Shared written by render(), View-visible + in
//       UI_RANGES, but never persisted or CC-set. Seeded to shared(0.0).
//   #[node]  /  #[node(each)]  /  #[node(init = sine())]
//       a sub-node whose set_sample_rate is forwarded (`each` -> per element).
//       init is only used by a generated new().
//   #[view]  a non-Shared field that still belongs in the View (a NamModelCycler).
//   (unannotated fields are internal state: excluded from View + set_sample_rate,
//    and only allowed under #[voice(new = manual)].)
//

use proc_macro::TokenStream;
use proc_macro2::{Literal, TokenStream as TokenStream2};
use quote::quote;
use syn::{
    parse_macro_input, Attribute, Data, DeriveInput, Expr, ExprClosure, ExprRange,
    Fields, Ident, LitStr, Type,
};

#[proc_macro_derive(Voice, attributes(voice, knob, live, node, view))]
pub fn derive_voice (input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand(input)
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

// ---- parsed field kinds ---------------------------------------------------

struct KnobField {
    ident:   Ident,
    ccs:     Vec<u8>,
    set:     Option<ExprClosure>,
    min:     Expr,
    max:     Expr,
    default: Option<Expr>,
}

struct LiveField {
    ident: Ident,
    min:   Expr,
    max:   Expr,
}

struct NodeField {
    ident: Ident,
    each:  bool,
    init:  Option<Expr>,
}

struct ViewField {
    ident: Ident,
    ty:    Type,
}

struct PlainField {
    ident: Ident,
}

#[derive(Default)]
struct Voice {
    knobs:  Vec<KnobField>,
    lives:  Vec<LiveField>,
    nodes:  Vec<NodeField>,
    views:  Vec<ViewField>,
    plains: Vec<PlainField>,
    thump:  Option<Ident>,
    sig:    Option<Ident>,
}

// ---- top-level expansion --------------------------------------------------

fn expand (input: DeriveInput) -> syn::Result<TokenStream2> {
    let name = input.ident.clone();

    let (index, label, manual_new, manual_thump) = parse_voice_attr(&input)?;

    let fields = match &input.data {
        Data::Struct(s) => match &s.fields {
            Fields::Named(f) => &f.named,
            _ => return Err(syn::Error::new_spanned(&input, "Voice derive needs named fields")),
        },
        _ => return Err(syn::Error::new_spanned(&input, "Voice can only be derived for structs")),
    };

    let mut v = Voice::default();
    for field in fields {
        let ident = field.ident.clone().unwrap();
        let ty = field.ty.clone();

        if let Some(attr) = find_attr(&field.attrs, "knob") {
            v.knobs.push(parse_knob(&ident, attr)?);
        } else if let Some(attr) = find_attr(&field.attrs, "live") {
            v.lives.push(parse_live(&ident, attr)?);
        } else if let Some(attr) = find_attr(&field.attrs, "node") {
            v.nodes.push(parse_node(&ident, attr)?);
        } else if find_attr(&field.attrs, "view").is_some() {
            v.views.push(ViewField { ident, ty });
        } else if ident == "thump" {
            v.thump = Some(ident);
        } else if ident == "sig" {
            v.sig = Some(ident);
        } else {
            v.plains.push(PlainField { ident });
        }
    }

    // thump is optional: a voice with no `thump: ThumpMod` field just never
    // pitch-thumps (freq passed through unchanged, thump_mult always 1.0) --
    // same generated behaviour as `thump = manual` on a voice that does have
    // the field but applies it itself.
    let thump = v.thump.clone();
    if v.sig.is_none() {
        return Err(syn::Error::new_spanned(&input, "Voice derive needs a `sig: SharedSignal` field"));
    }

    let view_struct = gen_view_struct(&name, &v);
    let view_impl   = gen_view_impl(&name, &v);
    let voice_inh   = gen_inherent(&name, &v, !manual_new, thump.as_ref())?;
    let voice_trait = gen_voice_trait(&name, &index, &label, &v, thump.as_ref(), manual_thump);

    Ok(quote! {
        #view_struct
        #view_impl
        #voice_inh
        #voice_trait
    })
}

// ---- attribute parsing ----------------------------------------------------

fn find_attr<'a> (attrs: &'a [Attribute], name: &str) -> Option<&'a Attribute> {
    attrs.iter().find(|a| a.path().is_ident(name))
}

fn parse_voice_attr (input: &DeriveInput) -> syn::Result<(Expr, LitStr, bool, bool)> {
    let attr = find_attr(&input.attrs, "voice")
        .ok_or_else(|| syn::Error::new_spanned(input, "missing #[voice(index=.., label=..)]"))?;

    let mut index = None;
    let mut label = None;
    let mut manual_new = false;
    let mut manual_thump = false;

    attr.parse_nested_meta(|meta| {
        if meta.path.is_ident("index") {
            index = Some(meta.value()?.parse::<Expr>()?);
        } else if meta.path.is_ident("label") {
            label = Some(meta.value()?.parse::<LitStr>()?);
        } else if meta.path.is_ident("new") {
            let v: Ident = meta.value()?.parse()?;
            manual_new = v == "manual";
        } else if meta.path.is_ident("thump") {
            let v: Ident = meta.value()?.parse()?;
            manual_thump = v == "manual";
        } else {
            return Err(meta.error("unknown #[voice] key"));
        }
        Ok(())
    })?;

    Ok((
        index.ok_or_else(|| syn::Error::new_spanned(attr, "#[voice] missing index"))?,
        label.ok_or_else(|| syn::Error::new_spanned(attr, "#[voice] missing label"))?,
        manual_new,
        manual_thump,
    ))
}

fn range_bounds (r: &ExprRange) -> syn::Result<(Expr, Expr)> {
    let start = r.start.as_ref().ok_or_else(|| syn::Error::new_spanned(r, "range needs a lower bound"))?;
    let end   = r.end.as_ref().ok_or_else(|| syn::Error::new_spanned(r, "range needs an upper bound"))?;
    Ok(((**start).clone(), (**end).clone()))
}

fn parse_knob (ident: &Ident, attr: &Attribute) -> syn::Result<KnobField> {
    let mut cc: Option<LitStr> = None;
    let mut range: Option<ExprRange> = None;
    let mut set: Option<ExprClosure> = None;
    let mut default: Option<Expr> = None;

    attr.parse_nested_meta(|meta| {
        if meta.path.is_ident("cc") {
            cc = Some(meta.value()?.parse()?);
        } else if meta.path.is_ident("range") {
            range = Some(meta.value()?.parse()?);
        } else if meta.path.is_ident("set") {
            set = Some(meta.value()?.parse()?);
        } else if meta.path.is_ident("default") {
            default = Some(meta.value()?.parse()?);
        } else {
            return Err(meta.error("unknown #[knob] key"));
        }
        Ok(())
    })?;

    let range = range.ok_or_else(|| syn::Error::new_spanned(attr, "#[knob] missing range"))?;
    let (min, max) = range_bounds(&range)?;

    let ccs = match &cc {
        Some(lit) => lit.value().split('|')
            .map(|s| s.trim().parse::<u8>()
                .map_err(|_| syn::Error::new_spanned(lit, "cc must be like \"6|44\"")))
            .collect::<syn::Result<Vec<_>>>()?,
        None => Vec::new(),
    };
    if !ccs.is_empty() && set.is_none() {
        set = Some(syn::parse_quote! { |v| v });
    }

    Ok(KnobField { ident: ident.clone(), ccs, set, min, max, default })
}

fn parse_live (ident: &Ident, attr: &Attribute) -> syn::Result<LiveField> {
    let mut range: Option<ExprRange> = None;
    attr.parse_nested_meta(|meta| {
        if meta.path.is_ident("range") {
            range = Some(meta.value()?.parse()?);
        } else {
            return Err(meta.error("unknown #[live] key"));
        }
        Ok(())
    })?;
    let range = range.ok_or_else(|| syn::Error::new_spanned(attr, "#[live] missing range"))?;
    let (min, max) = range_bounds(&range)?;
    Ok(LiveField { ident: ident.clone(), min, max })
}

fn parse_node (ident: &Ident, attr: &Attribute) -> syn::Result<NodeField> {
    let mut each = false;
    let mut init: Option<Expr> = None;
    // #[node] with no args is fine (parse_nested_meta simply runs zero times).
    if !matches!(attr.meta, syn::Meta::Path(_)) {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("each") {
                each = true;
            } else if meta.path.is_ident("init") {
                init = Some(meta.value()?.parse()?);
            } else {
                return Err(meta.error("unknown #[node] key"));
            }
            Ok(())
        })?;
    }
    Ok(NodeField { ident: ident.clone(), each, init })
}

// ---- codegen --------------------------------------------------------------

// Every field that appears in the read-only view: knobs, lives (both Shared),
// and view-passthrough fields.
fn gen_view_struct (name: &Ident, v: &Voice) -> TokenStream2 {
    let view_name = view_name(name);
    let knob_f = v.knobs.iter().map(|f| &f.ident);
    let live_f  = v.lives.iter().map(|f| &f.ident);
    let view_f  = v.views.iter().map(|f| &f.ident);
    let view_t  = v.views.iter().map(|f| &f.ty);
    quote! {
        #[derive(Clone)]
        pub struct #view_name {
            #( pub #knob_f: Shared, )*
            #( pub #live_f: Shared, )*
            #( pub #view_f: #view_t, )*
        }
    }
}

fn gen_view_impl (name: &Ident, v: &Voice) -> TokenStream2 {
    let view_name = view_name(name);
    let knob_names: Vec<_> = v.knobs.iter().map(|f| &f.ident).collect();

    quote! {
        impl crate::audio::voice::ViewFields for #view_name {
            fn fields (&self) -> Vec<(&'static str, f32)> {
                vec![
                    #( (stringify!(#knob_names), self.#knob_names.value()), )*
                ]
            }

            fn apply (&self, fields: &[(String, f32)]) {
                for (name, value) in fields {
                    match name.as_str() {
                        #( stringify!(#knob_names) => self.#knob_names.set_value(*value), )*
                        _ => {},
                    }
                }
            }
        }
    }
}

fn gen_inherent (name: &Ident, v: &Voice, generate_new: bool, thump: Option<&Ident>) -> syn::Result<TokenStream2> {
    let view_name = view_name(name);

    // view() clones every view-visible cell.
    let clone_knobs = v.knobs.iter().map(|f| &f.ident);
    let clone_lives  = v.lives.iter().map(|f| &f.ident);
    let clone_views  = v.views.iter().map(|f| &f.ident);
    let view_fn = quote! {
        pub fn view (&self) -> #view_name {
            #view_name {
                #( #clone_knobs: self.#clone_knobs.clone(), )*
                #( #clone_lives: self.#clone_lives.clone(), )*
                #( #clone_views: self.#clone_views.clone(), )*
            }
        }
    };

    // UI_RANGES: name -> (min, max) for knobs + lives.
    let range_entries = v.knobs.iter().map(|f| {
        let id = &f.ident; let (mn, mx) = (&f.min, &f.max);
        quote! { (stringify!(#id), #mn as f32, #mx as f32) }
    }).chain(v.lives.iter().map(|f| {
        let id = &f.ident; let (mn, mx) = (&f.min, &f.max);
        quote! { (stringify!(#id), #mn as f32, #mx as f32) }
    }));
    let ranges = quote! {
        pub const UI_RANGES: &'static [(&'static str, f32, f32)] = &[ #( #range_entries ),* ];
    };

    let new_fn = if generate_new {
        Some(gen_new(v, thump)?)
    } else {
        None
    };

    Ok(quote! {
        impl #name {
            #view_fn
            #ranges
            #new_fn
        }
    })
}

fn gen_new (v: &Voice, thump: Option<&Ident>) -> syn::Result<TokenStream2> {
    let thump = thump.ok_or_else(|| syn::Error::new(
        proc_macro2::Span::call_site(),
        "generated new() needs a `thump: ThumpMod` field (or #[voice(new = manual)])"))?;

    // Every field must be constructible from defaults/inits or a generated new
    // can't work -- point the author at #[voice(new = manual)] otherwise.
    if !v.plains.is_empty() {
        return Err(syn::Error::new(
            v.plains[0].ident.span(),
            "generated new() can't construct this field -- add #[voice(new = manual)] and write new() by hand",
        ));
    }
    if let Some(f) = v.views.first() {
        return Err(syn::Error::new(
            f.ident.span(),
            "generated new() can't construct a #[view] field -- add #[voice(new = manual)]",
        ));
    }

    let node_inits = v.nodes.iter().map(|f| {
        let id = &f.ident;
        match &f.init {
            Some(e) => Ok(quote! { #id: #e }),
            None => Err(syn::Error::new(id.span(),
                "generated new() needs #[node(init = ..)] here (or #[voice(new = manual)])")),
        }
    }).collect::<syn::Result<Vec<_>>>()?;

    let knob_inits = v.knobs.iter().map(|f| {
        let id = &f.ident;
        match &f.default {
            Some(e) => Ok(quote! { #id: shared(#e) }),
            None => Err(syn::Error::new(id.span(),
                "generated new() needs default=.. on this #[knob] (or #[voice(new = manual)])")),
        }
    }).collect::<syn::Result<Vec<_>>>()?;

    let live_inits = v.lives.iter().map(|f| {
        let id = &f.ident;
        quote! { #id: shared(0.0) }
    });

    Ok(quote! {
        pub fn new (thump_trigger: Shared, thump_peak: Shared, thump_decay: Shared, signal: SharedSignal) -> Self {
            Self {
                #( #node_inits, )*
                #( #knob_inits, )*
                #( #live_inits, )*
                #thump: ThumpMod::new(thump_trigger.clone(), thump_peak.clone(), thump_decay.clone()),
                sig: signal,
            }
        }
    })
}

// The whole `impl Voice for #name` block: index/name, tick (thump + render),
// set_sample_rate (forwards to every #[node] + thump), on_block_start,
// on_silence and apply_cc (all delegating to the author's VoiceDsp impl,
// except apply_cc which is fully generated from each #[knob]'s cc=..).
fn gen_voice_trait (name: &Ident, index: &Expr, label: &LitStr, v: &Voice, thump: Option<&Ident>, manual_thump: bool) -> TokenStream2 {
    let scalar_nodes = v.nodes.iter().filter(|f| !f.each).map(|f| &f.ident);
    let each_nodes   = v.nodes.iter().filter(|f| f.each).map(|f| &f.ident);

    // The common case: the macro applies thump to the incoming freq. thump =
    // manual voices (SwarmVoice chases an origin first, then thumps that) get
    // the raw freq + apply thump themselves inside render(). Voices with no
    // thump field at all (no pitch-thump modulation) get the same pass-
    // through body.
    let tick_body = match thump {
        Some(thump) if !manual_thump => quote! {
            let thump_mult = self.#thump.tick(self.sig.thump.value());
            let freq = freq * thump_mult;
            let out = crate::audio::voice::VoiceDsp::render(self, freq, thump_mult);
            (out[0], out[1])
        },
        _ => quote! {
            let out = crate::audio::voice::VoiceDsp::render(self, freq, 1.0);
            (out[0], out[1])
        },
    };

    let thump_set_sample_rate = thump.map(|thump| quote! {
        self.#thump.set_sample_rate(sample_rate);
    });

    let cc_arms = v.knobs.iter().filter(|f| !f.ccs.is_empty()).map(|f| {
        let id = &f.ident;
        let set = f.set.as_ref().unwrap();
        let ccs = f.ccs.iter().map(|n| Literal::u8_unsuffixed(*n));
        quote! { #( #ccs )|* => self.#id.set_value((#set)(value)), }
    });

    quote! {
        impl Voice for #name {
            fn index (&self) -> usize { #index }

            fn name (&self) -> &'static str { #label }

            // Whether this voice is the selected one is the caller's call
            // (see audio::engine::Engine) -- tick always renders.
            fn tick (&mut self, freq: f32) -> (f32, f32) {
                #tick_body
            }

            fn set_sample_rate (&mut self, sample_rate: f64) {
                #( self.#scalar_nodes.set_sample_rate(sample_rate); )*
                #( for n in self.#each_nodes.iter_mut() { n.set_sample_rate(sample_rate); } )*
                #thump_set_sample_rate
                crate::audio::voice::VoiceDsp::on_set_sample_rate(self, sample_rate);
            }

            fn on_block_start (&mut self, block_len: usize) {
                crate::audio::voice::VoiceDsp::on_block_start(self, block_len);
            }

            fn on_silence (&mut self) {
                crate::audio::voice::VoiceDsp::on_silence(self);
            }

            fn apply_cc (&mut self, cc: u8, value: f32) {
                let value = value.clamp(0.0, 1.0);
                match cc {
                    #( #cc_arms )*
                    _ => {},
                }
            }
        }
    }
}

fn view_name (name: &Ident) -> Ident {
    // ReeseVoice -> ReeseView, matching the hand-written names the rest of the
    // codebase already imports.
    let base = name.to_string();
    let base = base.strip_suffix("Voice").unwrap_or(&base);
    Ident::new(&format!("{}View", base), name.span())
}
