//! DECORATE parser (M5 bootstrap subset).

use crate::error::DecorateError;
use crate::include::expand_includes;
use crate::ir::{
    ActorDef, ClassDefIr, FlagOp, PropertyStmt, StateFlow, StateItem, StateLine, StatesBlock,
};
use ferrite_sc::{Punct, ScanOptions, Scanner, Tok};

/// Parse DECORATE after expanding `#include` via `resolve`.
pub fn parse_decorate_with_includes(
    src: &str,
    resolve: impl FnMut(&str) -> Result<String, DecorateError>,
) -> Result<ClassDefIr, DecorateError> {
    let expanded = expand_includes(src, resolve)?;
    parse_decorate(&expanded)
}

/// Parse a DECORATE source string into ClassDef IR.
pub fn parse_decorate(src: &str) -> Result<ClassDefIr, DecorateError> {
    let mut sc = Scanner::from_str(
        src,
        ScanOptions {
            c_mode: true,
            ..Default::default()
        },
    );
    let mut ir = ClassDefIr::default();
    loop {
        let tok = sc.next_token()?;
        match tok {
            Tok::Eof => break,
            Tok::Ident(kw) if Scanner::compare(kw, "actor") => {
                ir.actors.push(parse_actor(&mut sc)?);
            }
            Tok::Ident(kw) if Scanner::compare(kw, "const") || Scanner::compare(kw, "enum") => {
                // Skip until `;` for bootstrap.
                skip_until_semi(&mut sc)?;
            }
            other => {
                return Err(DecorateError::Message(format!(
                    "unexpected top-level token {other:?}"
                )));
            }
        }
    }
    Ok(ir)
}

fn parse_actor(sc: &mut Scanner<'_>) -> Result<ActorDef, DecorateError> {
    let name = sc.must_get_ident()?.to_owned();
    let mut parent = None;
    if sc.check_punct(Punct::Colon)? {
        parent = Some(sc.must_get_ident()?.to_owned());
    }
    let mut replaces = None;
    let peek = sc.next_token()?;
    match peek {
        Tok::Ident(kw) if Scanner::compare(kw, "replaces") => {
            replaces = Some(sc.must_get_ident()?.to_owned());
        }
        other => sc.unget(other),
    }
    let mut doomednum = None;
    let peek = sc.next_token()?;
    match peek {
        Tok::Int(n) => {
            #[allow(clippy::cast_possible_truncation)]
            {
                doomednum = Some(n as i32);
            }
        }
        other => sc.unget(other),
    }
    // optional `native`
    let peek = sc.next_token()?;
    match peek {
        Tok::Ident(kw) if Scanner::compare(kw, "native") => {}
        other => sc.unget(other),
    }
    if !sc.check_punct(Punct::LBrace)? {
        return Err(DecorateError::Message(
            "expected '{' after actor header".into(),
        ));
    }
    let mut flags = Vec::new();
    let mut properties = Vec::new();
    let mut states = None;
    loop {
        if sc.check_punct(Punct::RBrace)? {
            break;
        }
        let tok = sc.next_token()?;
        match tok {
            Tok::Punct(Punct::Plus) | Tok::Punct(Punct::Minus) => {
                let set = matches!(tok, Tok::Punct(Punct::Plus));
                let name = read_dotted_name(sc)?;
                flags.push(FlagOp { set, name });
            }
            Tok::Ident(kw) if Scanner::compare(kw, "states") => {
                states = Some(parse_states(sc)?);
            }
            Tok::Ident(kw) if Scanner::compare(kw, "clearflags") => {
                flags.push(FlagOp {
                    set: false,
                    name: "*".into(),
                });
            }
            Tok::Ident(name) => {
                // Property or keyword shortcut.
                if Scanner::compare(name, "monster") || Scanner::compare(name, "projectile") {
                    properties.push(PropertyStmt {
                        name: name.to_owned(),
                        args: Vec::new(),
                    });
                    continue;
                }
                let prop_name = read_dotted_suffix(sc, name)?;
                let args = read_prop_args(sc)?;
                properties.push(PropertyStmt {
                    name: prop_name,
                    args,
                });
            }
            Tok::Eof => {
                return Err(DecorateError::Message("unclosed actor body".into()));
            }
            other => {
                return Err(DecorateError::Message(format!(
                    "unexpected token in actor body: {other:?}"
                )));
            }
        }
    }
    Ok(ActorDef {
        name,
        parent,
        replaces,
        doomednum,
        flags,
        properties,
        states,
    })
}

fn parse_states(sc: &mut Scanner<'_>) -> Result<StatesBlock, DecorateError> {
    if !sc.check_punct(Punct::LBrace)? {
        return Err(DecorateError::Message("expected '{' after states".into()));
    }
    let mut items = Vec::new();
    loop {
        if sc.check_punct(Punct::RBrace)? {
            break;
        }
        let tok = sc.next_token()?;
        match tok {
            Tok::Ident(kw) if Scanner::compare(kw, "loop") => {
                items.push(StateItem::Flow(StateFlow::Loop));
            }
            Tok::Ident(kw) if Scanner::compare(kw, "stop") => {
                items.push(StateItem::Flow(StateFlow::Stop));
            }
            Tok::Ident(kw) if Scanner::compare(kw, "wait") => {
                items.push(StateItem::Flow(StateFlow::Wait));
            }
            Tok::Ident(kw) if Scanner::compare(kw, "fail") => {
                items.push(StateItem::Flow(StateFlow::Fail));
            }
            Tok::Ident(kw) if Scanner::compare(kw, "goto") => {
                let label = read_goto_label(sc)?;
                let mut offset = 0;
                if sc.check_punct(Punct::Plus)? {
                    offset = sc.must_get_number()? as i32;
                }
                items.push(StateItem::Flow(StateFlow::Goto { label, offset }));
            }
            Tok::Ident(name) => {
                // Label `Name:` / `Death.Fire:` or state line starting with sprite.
                let mut label = name.to_owned();
                while sc.check_punct(Punct::Dot)? {
                    label.push('.');
                    label.push_str(sc.must_get_ident()?);
                }
                if sc.check_punct(Punct::Colon)? {
                    items.push(StateItem::Label(label));
                } else {
                    // State line: SPRITE FRAMES DURATION …
                    // Dotted idents are labels only; sprites are undotted 4-letter names.
                    if label.contains('.') {
                        return Err(DecorateError::Message(format!(
                            "expected ':' after label {label}"
                        )));
                    }
                    let sprite = label;
                    let frames = match sc.next_token()? {
                        Tok::Ident(f) => f.to_owned(),
                        Tok::String(s) => s.into_owned(),
                        other => {
                            return Err(DecorateError::Message(format!(
                                "expected frames, got {other:?}"
                            )));
                        }
                    };
                    let (duration, random_range) = read_duration(sc)?;
                    let mut modifiers = Vec::new();
                    if let Some((lo, hi)) = random_range {
                        modifiers.push(format!("random({lo},{hi})"));
                    }
                    let mut action = None;
                    let mut action_args = Vec::new();
                    let mut extra_actions: Vec<(String, Vec<String>)> = Vec::new();
                    loop {
                        let t = sc.next_token()?;
                        match t {
                            Tok::Punct(Punct::Semi) => break,
                            Tok::Punct(Punct::LBrace) => {
                                let mut stmts = read_anon_actions(sc)?;
                                if action.is_none() {
                                    if let Some((name, args)) = stmts.first() {
                                        action = Some(name.clone());
                                        action_args = args.clone();
                                    }
                                    if stmts.len() > 1 {
                                        extra_actions.extend(stmts.drain(1..));
                                    }
                                } else {
                                    extra_actions.extend(stmts);
                                }
                            }
                            Tok::Ident(m)
                                if Scanner::compare(m, "bright")
                                    || Scanner::compare(m, "fast")
                                    || Scanner::compare(m, "slow")
                                    || Scanner::compare(m, "nodelay")
                                    || Scanner::compare(m, "canraise") =>
                            {
                                modifiers.push(m.to_owned());
                            }
                            Tok::Ident(a)
                                if Scanner::compare(a, "loop")
                                    || Scanner::compare(a, "stop")
                                    || Scanner::compare(a, "wait")
                                    || Scanner::compare(a, "fail")
                                    || Scanner::compare(a, "goto") =>
                            {
                                sc.unget(Tok::Ident(a));
                                break;
                            }
                            Tok::Ident(a)
                                if a.starts_with("A_")
                                    || a.starts_with("a_")
                                    || Scanner::compare(a, "ACS_Execute") =>
                            {
                                action = Some(a.to_owned());
                                if sc.check_punct(Punct::LParen)? {
                                    action_args = read_action_args(sc)?;
                                }
                            }
                            Tok::Ident(a) => {
                                // Likely the next sprite name — end this line.
                                sc.unget(Tok::Ident(a));
                                break;
                            }
                            Tok::Eof => break,
                            other => {
                                sc.unget(other);
                                break;
                            }
                        }
                    }
                    items.push(StateItem::Line(StateLine {
                        sprite: sprite.clone(),
                        frames: frames.clone(),
                        duration,
                        modifiers,
                        action,
                        action_args,
                    }));
                    let extra_frame = frames.chars().next().unwrap_or('A').to_string();
                    for (name, args) in extra_actions {
                        items.push(StateItem::Line(StateLine {
                            sprite: sprite.clone(),
                            frames: extra_frame.clone(),
                            duration: 0,
                            modifiers: Vec::new(),
                            action: Some(name),
                            action_args: args,
                        }));
                    }
                }
            }
            Tok::Eof => {
                return Err(DecorateError::Message("unclosed states".into()));
            }
            other => {
                return Err(DecorateError::Message(format!(
                    "unexpected in states: {other:?}"
                )));
            }
        }
    }
    Ok(StatesBlock { items })
}

fn read_dotted_name(sc: &mut Scanner<'_>) -> Result<String, DecorateError> {
    let mut name = sc.must_get_ident()?.to_owned();
    while sc.check_punct(Punct::Dot)? {
        name.push('.');
        name.push_str(sc.must_get_ident()?);
    }
    Ok(name)
}

fn read_dotted_suffix(sc: &mut Scanner<'_>, first: &str) -> Result<String, DecorateError> {
    let mut name = first.to_owned();
    while sc.check_punct(Punct::Dot)? {
        name.push('.');
        name.push_str(sc.must_get_ident()?);
    }
    Ok(name)
}

fn read_prop_args(sc: &mut Scanner<'_>) -> Result<Vec<String>, DecorateError> {
    let mut args = Vec::new();
    // `true` at start and after each comma — next value token is an arg.
    let mut need_value = true;
    loop {
        let tok = sc.next_token()?;
        match tok {
            Tok::Punct(Punct::Semi) => break,
            Tok::Eof => {
                sc.unget(tok);
                break;
            }
            Tok::Punct(Punct::Comma) => {
                need_value = true;
            }
            Tok::Ident(s) => {
                // Bare idents are args only when a value is expected
                // (`Game Doom`, `DropItem Clip, 256`). Otherwise the ident is
                // the next property (`Health 20` then `Radius 20`).
                if !need_value {
                    sc.unget(Tok::Ident(s));
                    break;
                }
                args.push(s.to_owned());
                need_value = false;
            }
            Tok::String(s) => {
                if !need_value {
                    sc.unget(Tok::String(s));
                    break;
                }
                args.push(s.into_owned());
                need_value = false;
            }
            Tok::Int(n) => {
                if !need_value {
                    sc.unget(Tok::Int(n));
                    break;
                }
                args.push(n.to_string());
                need_value = false;
            }
            Tok::Float(f) => {
                if !need_value {
                    sc.unget(Tok::Float(f));
                    break;
                }
                args.push(f.to_string());
                need_value = false;
            }
            Tok::Punct(Punct::RBrace) | Tok::Punct(Punct::Plus) => {
                sc.unget(tok);
                break;
            }
            Tok::Punct(Punct::Minus) => {
                if !need_value {
                    sc.unget(Tok::Punct(Punct::Minus));
                    break;
                }
                let next = sc.next_token()?;
                match next {
                    Tok::Int(n) => {
                        args.push((-n).to_string());
                        need_value = false;
                    }
                    Tok::Float(f) => {
                        args.push((-f).to_string());
                        need_value = false;
                    }
                    other => {
                        sc.unget(other);
                        sc.unget(Tok::Punct(Punct::Minus));
                        break;
                    }
                }
            }
            other => {
                sc.unget(other);
                break;
            }
        }
    }
    Ok(args)
}

fn read_goto_label(sc: &mut Scanner<'_>) -> Result<String, DecorateError> {
    let mut label = sc.must_get_ident()?.to_owned();
    if sc.check_punct(Punct::Colon)? {
        // Super::Label — we already ate one colon; expect another for ::
        if sc.check_punct(Punct::Colon)? {
            label.push_str("::");
            label.push_str(sc.must_get_ident()?);
        }
    }
    while sc.check_punct(Punct::Dot)? {
        label.push('.');
        label.push_str(sc.must_get_ident()?);
    }
    Ok(label)
}

fn read_i32(sc: &mut Scanner<'_>) -> Result<i32, DecorateError> {
    let neg = sc.check_punct(Punct::Minus)?;
    let n = sc.must_get_number()?;
    #[allow(clippy::cast_possible_truncation)]
    let v = n as i32;
    Ok(if neg { -v } else { v })
}

fn read_duration(sc: &mut Scanner<'_>) -> Result<(i32, Option<(i32, i32)>), DecorateError> {
    let tok = sc.next_token()?;
    match tok {
        Tok::Ident(s) if Scanner::compare(s, "random") => {
            if !sc.check_punct(Punct::LParen)? {
                return Err(DecorateError::Message("expected '(' after random".into()));
            }
            let lo = read_i32(sc)?;
            if !sc.check_punct(Punct::Comma)? {
                return Err(DecorateError::Message("expected ',' in random()".into()));
            }
            let hi = read_i32(sc)?;
            if !sc.check_punct(Punct::RParen)? {
                return Err(DecorateError::Message("expected ')' after random()".into()));
            }
            Ok((lo, Some((lo, hi))))
        }
        Tok::Punct(Punct::LParen) => {
            let v = eval_int_expr(sc)?;
            if !sc.check_punct(Punct::RParen)? {
                return Err(DecorateError::Message(
                    "expected ')' after duration expression".into(),
                ));
            }
            Ok((v, None))
        }
        other => {
            sc.unget(other);
            Ok((read_i32(sc)?, None))
        }
    }
}

/// Constant integer expression (`+` `-` `*` `/` and parentheses).
fn eval_int_expr(sc: &mut Scanner<'_>) -> Result<i32, DecorateError> {
    eval_int_sum(sc)
}

fn eval_int_sum(sc: &mut Scanner<'_>) -> Result<i32, DecorateError> {
    let mut v = eval_int_term(sc)?;
    loop {
        if sc.check_punct(Punct::Plus)? {
            v = v.saturating_add(eval_int_term(sc)?);
        } else if sc.check_punct(Punct::Minus)? {
            v = v.saturating_sub(eval_int_term(sc)?);
        } else {
            break;
        }
    }
    Ok(v)
}

fn eval_int_term(sc: &mut Scanner<'_>) -> Result<i32, DecorateError> {
    let mut v = eval_int_primary(sc)?;
    loop {
        if sc.check_punct(Punct::Star)? {
            v = v.saturating_mul(eval_int_primary(sc)?);
        } else if sc.check_punct(Punct::Slash)? {
            let d = eval_int_primary(sc)?;
            if d == 0 {
                return Err(DecorateError::Message("division by zero".into()));
            }
            v /= d;
        } else {
            break;
        }
    }
    Ok(v)
}

fn eval_int_primary(sc: &mut Scanner<'_>) -> Result<i32, DecorateError> {
    if sc.check_punct(Punct::LParen)? {
        let v = eval_int_sum(sc)?;
        if !sc.check_punct(Punct::RParen)? {
            return Err(DecorateError::Message("expected ')' in expression".into()));
        }
        return Ok(v);
    }
    read_i32(sc)
}

/// `{ A_Foo; A_Bar(1, 2); }` — opening `{` already consumed.
///
/// Bootstrap DecoBlock: `return;` / `return state("L")` / `return "L"`, and
/// `if (health < N) { … }` (constant-fold or health-threshold patterns).
fn read_anon_actions(sc: &mut Scanner<'_>) -> Result<Vec<(String, Vec<String>)>, DecorateError> {
    let mut out = Vec::new();
    loop {
        if sc.check_punct(Punct::RBrace)? {
            break;
        }
        let tok = sc.next_token()?;
        match tok {
            Tok::Ident(name) if Scanner::compare(name, "return") => {
                if let Some(stmt) = read_return_stmt(sc)? {
                    out.push(stmt);
                }
            }
            Tok::Ident(name) if Scanner::compare(name, "if") => {
                out.extend(read_if_stmt(sc)?);
            }
            Tok::Ident(name) => {
                let mut args = Vec::new();
                if sc.check_punct(Punct::LParen)? {
                    args = read_action_args(sc)?;
                }
                let _ = sc.check_punct(Punct::Semi)?;
                out.push((name.to_owned(), args));
            }
            Tok::Punct(Punct::Semi) => {}
            Tok::Eof => {
                return Err(DecorateError::Message("unclosed anonymous function".into()));
            }
            other => {
                return Err(DecorateError::Message(format!(
                    "unexpected in anonymous function: {other:?}"
                )));
            }
        }
    }
    Ok(out)
}

/// `return;` → none; `return state("L")` / `return "L"` → `A_Jump(256, L)`.
fn read_return_stmt(sc: &mut Scanner<'_>) -> Result<Option<(String, Vec<String>)>, DecorateError> {
    if sc.check_punct(Punct::Semi)? {
        return Ok(None);
    }
    let label = read_state_return_label(sc)?;
    let _ = sc.check_punct(Punct::Semi)?;
    if label.is_empty() {
        return Ok(None);
    }
    Ok(Some(("A_Jump".into(), vec!["256".into(), label])))
}

fn read_state_return_label(sc: &mut Scanner<'_>) -> Result<String, DecorateError> {
    let tok = sc.next_token()?;
    match tok {
        Tok::String(s) => Ok(s.into_owned()),
        Tok::Ident(name) if Scanner::compare(name, "state") => {
            if !sc.check_punct(Punct::LParen)? {
                return Err(DecorateError::Message(
                    "expected '(' after state in return".into(),
                ));
            }
            let label = match sc.next_token()? {
                Tok::String(s) => s.into_owned(),
                Tok::Ident(id) => id.to_owned(),
                other => {
                    return Err(DecorateError::Message(format!(
                        "expected state label, got {other:?}"
                    )));
                }
            };
            if !sc.check_punct(Punct::RParen)? {
                return Err(DecorateError::Message(
                    "expected ')' after state(...)".into(),
                ));
            }
            Ok(label)
        }
        Tok::Ident(name) => Ok(name.to_owned()),
        other => Err(DecorateError::Message(format!(
            "unexpected after return: {other:?}"
        ))),
    }
}

/// `if (cond) { body } [else { body2 }]` — health-threshold and constant folds.
fn read_if_stmt(sc: &mut Scanner<'_>) -> Result<Vec<(String, Vec<String>)>, DecorateError> {
    if !sc.check_punct(Punct::LParen)? {
        return Err(DecorateError::Message("expected '(' after if".into()));
    }
    let cond_tokens = read_balanced_paren_inner(sc)?;
    if !sc.check_punct(Punct::LBrace)? {
        return Err(DecorateError::Message("expected '{' after if (...)".into()));
    }
    let then_body = read_anon_actions(sc)?;
    let else_body = {
        let t = sc.next_token()?;
        match t {
            Tok::Ident(name) if Scanner::compare(name, "else") => {
                if !sc.check_punct(Punct::LBrace)? {
                    return Err(DecorateError::Message("expected '{' after else".into()));
                }
                read_anon_actions(sc)?
            }
            other => {
                sc.unget(other);
                Vec::new()
            }
        }
    };

    if let Some(v) = try_fold_cond_bool(&cond_tokens) {
        return Ok(if v { then_body } else { else_body });
    }
    // `health op N` (`<`/`<=` keep JumpIfHealthLower path).
    if let Some((cmp, value)) = parse_named_int_cmp(&cond_tokens, "health") {
        return Ok(match cmp {
            4 => flatten_health_if(value, then_body, else_body),
            5 => flatten_health_if(value.saturating_add(1), then_body, else_body),
            _ => flatten_health_cmp_if(cmp, value, then_body, else_body),
        });
    }
    // `CountInv("X")` / `inventory("X")` [op N]
    if let Some((item, amount)) = parse_inventory_threshold(&cond_tokens) {
        return Ok(flatten_inventory_if(&item, amount, then_body, else_body));
    }
    // `args[n]` / `args[n] op N`
    if let Some((index, cmp, value)) = parse_args_threshold(&cond_tokens) {
        return Ok(flatten_args_if(index, cmp, value, then_body, else_body));
    }
    // `speed op N`
    if let Some((cmp, value)) = parse_speed_threshold(&cond_tokens) {
        return Ok(flatten_speed_if(cmp, value, then_body, else_body));
    }
    // `angle op N` (degrees)
    if let Some((cmp, value)) = parse_angle_threshold(&cond_tokens) {
        return Ok(flatten_angle_if(cmp, value, then_body, else_body));
    }
    // `pitch op N` (degrees)
    if let Some((cmp, value)) = parse_pitch_threshold(&cond_tokens) {
        return Ok(flatten_pitch_if(cmp, value, then_body, else_body));
    }
    // `tid op N`
    if let Some((cmp, value)) = parse_tid_threshold(&cond_tokens) {
        return Ok(flatten_tid_if(cmp, value, then_body, else_body));
    }
    // `mass op N`
    if let Some((cmp, value)) = parse_mass_threshold(&cond_tokens) {
        return Ok(flatten_mass_if(cmp, value, then_body, else_body));
    }
    // `height op N` (map units)
    if let Some((cmp, value)) = parse_height_threshold(&cond_tokens) {
        return Ok(flatten_height_if(cmp, value, then_body, else_body));
    }
    // `radius op N` (map units)
    if let Some((cmp, value)) = parse_radius_threshold(&cond_tokens) {
        return Ok(flatten_radius_if(cmp, value, then_body, else_body));
    }
    // `velx` / `vely` / `velz` op N
    if let Some((axis, cmp, value)) = parse_vel_threshold(&cond_tokens) {
        return Ok(flatten_vel_if(axis, cmp, value, then_body, else_body));
    }
    // `alpha op N` (scaled ×1000)
    if let Some((cmp, value)) = parse_named_scaled_cmp(&cond_tokens, "alpha", 1000.0) {
        return Ok(flatten_cmp_skip_if(
            "A_SkipUnlessAlpha",
            cmp,
            value,
            then_body,
            else_body,
        ));
    }
    // `gravity op N` (scaled ×1000)
    if let Some((cmp, value)) = parse_named_scaled_cmp(&cond_tokens, "gravity", 1000.0) {
        return Ok(flatten_cmp_skip_if(
            "A_SkipUnlessGravity",
            cmp,
            value,
            then_body,
            else_body,
        ));
    }
    // `roll op N` (degrees)
    if let Some((cmp, value)) = parse_named_int_cmp(&cond_tokens, "roll") {
        return Ok(flatten_cmp_skip_if(
            "A_SkipUnlessRoll",
            cmp,
            value,
            then_body,
            else_body,
        ));
    }
    // `damage op N`
    if let Some((cmp, value)) = parse_named_int_cmp(&cond_tokens, "damage") {
        return Ok(flatten_cmp_skip_if(
            "A_SkipUnlessDamage",
            cmp,
            value,
            then_body,
            else_body,
        ));
    }
    // `score op N`
    if let Some((cmp, value)) = parse_named_int_cmp(&cond_tokens, "score") {
        return Ok(flatten_cmp_skip_if(
            "A_SkipUnlessScore",
            cmp,
            value,
            then_body,
            else_body,
        ));
    }
    // `special op N`
    if let Some((cmp, value)) = parse_named_int_cmp(&cond_tokens, "special") {
        return Ok(flatten_cmp_skip_if(
            "A_SkipUnlessSpecial",
            cmp,
            value,
            then_body,
            else_body,
        ));
    }
    // `friction op N` (scaled ×1000)
    if let Some((cmp, value)) = parse_named_scaled_cmp(&cond_tokens, "friction", 1000.0) {
        return Ok(flatten_cmp_skip_if(
            "A_SkipUnlessFriction",
            cmp,
            value,
            then_body,
            else_body,
        ));
    }
    // `damagefactor op N` (scaled ×1000)
    if let Some((cmp, value)) = parse_named_scaled_cmp(&cond_tokens, "damagefactor", 1000.0) {
        return Ok(flatten_cmp_skip_if(
            "A_SkipUnlessDamageFactor",
            cmp,
            value,
            then_body,
            else_body,
        ));
    }
    // `accuracy op N` / `stamina op N`
    if let Some((cmp, value)) = parse_named_int_cmp(&cond_tokens, "accuracy") {
        return Ok(flatten_cmp_skip_if(
            "A_SkipUnlessAccuracy",
            cmp,
            value,
            then_body,
            else_body,
        ));
    }
    if let Some((cmp, value)) = parse_named_int_cmp(&cond_tokens, "stamina") {
        return Ok(flatten_cmp_skip_if(
            "A_SkipUnlessStamina",
            cmp,
            value,
            then_body,
            else_body,
        ));
    }
    // `team op N` (designated team)
    if let Some((cmp, value)) = parse_named_int_cmp(&cond_tokens, "team") {
        return Ok(flatten_cmp_skip_if(
            "A_SkipUnlessTeam",
            cmp,
            value,
            then_body,
            else_body,
        ));
    }
    // `meleerange op N` / `viewheight op N` (map units)
    if let Some((cmp, value)) = parse_named_int_cmp(&cond_tokens, "meleerange") {
        return Ok(flatten_cmp_skip_if(
            "A_SkipUnlessMeleeRange",
            cmp,
            value,
            then_body,
            else_body,
        ));
    }
    if let Some((cmp, value)) = parse_named_int_cmp(&cond_tokens, "jumpz") {
        return Ok(flatten_cmp_skip_if(
            "A_SkipUnlessJumpZ",
            cmp,
            value,
            then_body,
            else_body,
        ));
    }
    if let Some((cmp, value)) = parse_named_int_cmp(&cond_tokens, "viewheight") {
        return Ok(flatten_cmp_skip_if(
            "A_SkipUnlessViewHeight",
            cmp,
            value,
            then_body,
            else_body,
        ));
    }
    if let Some((cmp, value)) = parse_named_int_cmp(&cond_tokens, "attackzoffset") {
        return Ok(flatten_cmp_skip_if(
            "A_SkipUnlessAttackZOffset",
            cmp,
            value,
            then_body,
            else_body,
        ));
    }
    if let Some((cmp, value)) = parse_named_int_cmp(&cond_tokens, "maxstepheight") {
        return Ok(flatten_cmp_skip_if(
            "A_SkipUnlessMaxStepHeight",
            cmp,
            value,
            then_body,
            else_body,
        ));
    }
    if let Some((cmp, value)) = parse_named_int_cmp(&cond_tokens, "maxdropoffheight") {
        return Ok(flatten_cmp_skip_if(
            "A_SkipUnlessMaxDropOffHeight",
            cmp,
            value,
            then_body,
            else_body,
        ));
    }
    if let Some((cmp, value)) = parse_named_int_cmp(&cond_tokens, "floatspeed") {
        return Ok(flatten_cmp_skip_if(
            "A_SkipUnlessFloatSpeed",
            cmp,
            value,
            then_body,
            else_body,
        ));
    }
    if let Some((cmp, value)) = parse_named_int_cmp(&cond_tokens, "fastspeed") {
        return Ok(flatten_cmp_skip_if(
            "A_SkipUnlessFastSpeed",
            cmp,
            value,
            then_body,
            else_body,
        ));
    }
    if let Some((cmp, value)) = parse_named_int_cmp(&cond_tokens, "cameraheight") {
        return Ok(flatten_cmp_skip_if(
            "A_SkipUnlessCameraHeight",
            cmp,
            value,
            then_body,
            else_body,
        ));
    }
    if let Some((cmp, value)) = parse_named_int_cmp(&cond_tokens, "camerafov") {
        return Ok(flatten_cmp_skip_if(
            "A_SkipUnlessCameraFOV",
            cmp,
            value,
            then_body,
            else_body,
        ));
    }
    // `species == "Name"` / `!=`
    if let Some((cmp, value)) = parse_named_string_eq(&cond_tokens, "species") {
        return Ok(flatten_string_skip_if(
            "A_SkipUnlessSpecies",
            cmp,
            &value,
            then_body,
            else_body,
        ));
    }
    // `reactiontime op N`
    if let Some((cmp, value)) = parse_named_int_cmp(&cond_tokens, "reactiontime") {
        return Ok(flatten_cmp_skip_if(
            "A_SkipUnlessReactionTime",
            cmp,
            value,
            then_body,
            else_body,
        ));
    }
    // `painthreshold op N`
    if let Some((cmp, value)) = parse_named_int_cmp(&cond_tokens, "painthreshold") {
        return Ok(flatten_cmp_skip_if(
            "A_SkipUnlessPainThreshold",
            cmp,
            value,
            then_body,
            else_body,
        ));
    }
    // `threshold op N` (infight / target stickiness)
    if let Some((cmp, value)) = parse_named_int_cmp(&cond_tokens, "threshold") {
        return Ok(flatten_cmp_skip_if(
            "A_SkipUnlessThreshold",
            cmp,
            value,
            then_body,
            else_body,
        ));
    }
    // `user_foo` / `user_foo op N`
    if let Some((name, cmp, value)) = parse_user_var_threshold(&cond_tokens) {
        return Ok(flatten_user_var_if(&name, cmp, value, then_body, else_body));
    }
    // Unrecognised condition — keep then_body so the actor still loads.
    Ok(then_body)
}

fn flatten_health_cmp_if(
    cmp: i32,
    value: i32,
    then_body: Vec<(String, Vec<String>)>,
    else_body: Vec<(String, Vec<String>)>,
) -> Vec<(String, Vec<String>)> {
    flatten_cmp_skip_if("A_SkipUnlessHealth", cmp, value, then_body, else_body)
}

fn flatten_health_if(
    thresh: i32,
    then_body: Vec<(String, Vec<String>)>,
    else_body: Vec<(String, Vec<String>)>,
) -> Vec<(String, Vec<String>)> {
    if else_body.is_empty()
        && then_body.len() == 1
        && then_body[0].0.eq_ignore_ascii_case("A_Jump")
        && then_body[0].1.first().is_some_and(|c| c == "256")
        && then_body[0].1.get(1).is_some()
    {
        let label = then_body[0].1[1].clone();
        return vec![(
            "A_JumpIfHealthLower".into(),
            vec![thresh.to_string(), label],
        )];
    }
    if else_body.is_empty() && !then_body.is_empty() {
        let mut out = vec![(
            "A_SkipUnlessHealthLower".into(),
            vec![thresh.to_string(), then_body.len().to_string()],
        )];
        out.extend(then_body);
        return out;
    }
    if !else_body.is_empty() {
        let else_len = else_body.len();
        let mut out = vec![(
            "A_SkipUnlessHealthLower".into(),
            vec![thresh.to_string(), (then_body.len() + 1).to_string()],
        )];
        out.extend(then_body);
        out.push(("A_SkipAlways".into(), vec![else_len.to_string()]));
        out.extend(else_body);
        return out;
    }
    Vec::new()
}

fn flatten_args_if(
    index: i32,
    cmp: i32,
    value: i32,
    then_body: Vec<(String, Vec<String>)>,
    else_body: Vec<(String, Vec<String>)>,
) -> Vec<(String, Vec<String>)> {
    let skip_args = |count: usize| {
        (
            "A_SkipUnlessArg".into(),
            vec![
                index.to_string(),
                cmp.to_string(),
                value.to_string(),
                count.to_string(),
            ],
        )
    };
    if else_body.is_empty() && !then_body.is_empty() {
        let mut out = vec![skip_args(then_body.len())];
        out.extend(then_body);
        return out;
    }
    if !else_body.is_empty() {
        let else_len = else_body.len();
        let mut out = vec![skip_args(then_body.len() + 1)];
        out.extend(then_body);
        out.push(("A_SkipAlways".into(), vec![else_len.to_string()]));
        out.extend(else_body);
        return out;
    }
    Vec::new()
}

fn flatten_inventory_if(
    item: &str,
    amount: i32,
    then_body: Vec<(String, Vec<String>)>,
    else_body: Vec<(String, Vec<String>)>,
) -> Vec<(String, Vec<String>)> {
    if else_body.is_empty()
        && then_body.len() == 1
        && then_body[0].0.eq_ignore_ascii_case("A_Jump")
        && then_body[0].1.first().is_some_and(|c| c == "256")
        && then_body[0].1.get(1).is_some()
    {
        let label = then_body[0].1[1].clone();
        return vec![(
            "A_JumpIfInventory".into(),
            vec![item.to_owned(), amount.to_string(), label],
        )];
    }
    if else_body.is_empty() && !then_body.is_empty() {
        let mut out = vec![(
            "A_SkipUnlessInventory".into(),
            vec![
                format!("\"{item}\""),
                amount.to_string(),
                then_body.len().to_string(),
            ],
        )];
        out.extend(then_body);
        return out;
    }
    if !else_body.is_empty() {
        let else_len = else_body.len();
        let mut out = vec![(
            "A_SkipUnlessInventory".into(),
            vec![
                format!("\"{item}\""),
                amount.to_string(),
                (then_body.len() + 1).to_string(),
            ],
        )];
        out.extend(then_body);
        out.push(("A_SkipAlways".into(), vec![else_len.to_string()]));
        out.extend(else_body);
        return out;
    }
    Vec::new()
}

/// Condition tokens owned as strings / ints so we don't borrow from the scanner.
#[derive(Clone, Debug)]
enum CondAtom {
    Ident(String),
    String(String),
    Int(i64),
    Float(f64),
    Punct(Punct),
}

fn read_balanced_paren_inner(sc: &mut Scanner<'_>) -> Result<Vec<CondAtom>, DecorateError> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    loop {
        let t = sc.next_token()?;
        match t {
            Tok::Punct(Punct::LParen) => {
                depth += 1;
                out.push(CondAtom::Punct(Punct::LParen));
            }
            Tok::Punct(Punct::RParen) if depth == 0 => break,
            Tok::Punct(Punct::RParen) => {
                depth -= 1;
                out.push(CondAtom::Punct(Punct::RParen));
            }
            Tok::Eof => {
                return Err(DecorateError::Message("unclosed if condition".into()));
            }
            Tok::Ident(s) => out.push(CondAtom::Ident(s.to_owned())),
            Tok::String(s) => out.push(CondAtom::String(s.into_owned())),
            Tok::Int(n) => out.push(CondAtom::Int(n)),
            Tok::Float(f) => out.push(CondAtom::Float(f)),
            Tok::Punct(p) => out.push(CondAtom::Punct(p)),
        }
    }
    Ok(out)
}

fn try_fold_cond_bool(tokens: &[CondAtom]) -> Option<bool> {
    if tokens.len() != 1 {
        return None;
    }
    match &tokens[0] {
        CondAtom::Int(n) => Some(*n != 0),
        CondAtom::Float(f) => Some(*f != 0.0),
        CondAtom::Ident(s) if Scanner::compare(s, "true") => Some(true),
        CondAtom::Ident(s) if Scanner::compare(s, "false") => Some(false),
        _ => None,
    }
}

/// `args[n]` / `args[n] op N` → (index, cmp, value).
/// cmp: 0=`==`, 1=`!=`, 2=`>`, 3=`>=`, 4=`<`, 5=`<=`.
fn parse_args_threshold(tokens: &[CondAtom]) -> Option<(i32, i32, i32)> {
    // args [ n ]
    if tokens.len() < 4 {
        return None;
    }
    let CondAtom::Ident(name) = &tokens[0] else {
        return None;
    };
    if !Scanner::compare(name, "args") {
        return None;
    }
    if !matches!(&tokens[1], CondAtom::Punct(Punct::LBracket)) {
        return None;
    }
    let CondAtom::Int(idx) = &tokens[2] else {
        return None;
    };
    let index = i32::try_from(*idx).ok()?;
    if !(0..=4).contains(&index) {
        return None;
    }
    if !matches!(&tokens[3], CondAtom::Punct(Punct::RBracket)) {
        return None;
    }
    if tokens.len() == 4 {
        // truthy: args[n] != 0
        return Some((index, 1, 0));
    }
    if tokens.len() != 6 {
        return None;
    }
    let CondAtom::Int(n) = &tokens[5] else {
        return None;
    };
    let value = i32::try_from(*n).ok()?;
    let cmp = match &tokens[4] {
        CondAtom::Punct(Punct::Eq | Punct::EqEq) => 0,
        CondAtom::Punct(Punct::NotEq) => 1,
        CondAtom::Punct(Punct::Gt) => 2,
        CondAtom::Punct(Punct::GtEq) => 3,
        CondAtom::Punct(Punct::Lt) => 4,
        CondAtom::Punct(Punct::LtEq) => 5,
        _ => return None,
    };
    Some((index, cmp, value))
}

/// `CountInv("X")` / `inventory("X")` [op N] → (item, min amount for true).
/// `speed op N` → (cmp, value). cmp codes match `args` (0=`==` … 5=`<=`).
fn parse_speed_threshold(tokens: &[CondAtom]) -> Option<(i32, i32)> {
    parse_named_int_cmp(tokens, "speed")
}

/// `angle op N` (degrees) → (cmp, value).
fn parse_angle_threshold(tokens: &[CondAtom]) -> Option<(i32, i32)> {
    parse_named_int_cmp(tokens, "angle")
}

/// `pitch op N` (degrees) → (cmp, value).
fn parse_pitch_threshold(tokens: &[CondAtom]) -> Option<(i32, i32)> {
    parse_named_int_cmp(tokens, "pitch")
}

/// `tid op N` → (cmp, value).
fn parse_tid_threshold(tokens: &[CondAtom]) -> Option<(i32, i32)> {
    parse_named_int_cmp(tokens, "tid")
}

/// `mass op N` → (cmp, value).
fn parse_mass_threshold(tokens: &[CondAtom]) -> Option<(i32, i32)> {
    parse_named_int_cmp(tokens, "mass")
}

/// `height op N` (map units) → (cmp, value).
fn parse_height_threshold(tokens: &[CondAtom]) -> Option<(i32, i32)> {
    parse_named_int_cmp(tokens, "height")
}

/// `radius op N` (map units) → (cmp, value).
fn parse_radius_threshold(tokens: &[CondAtom]) -> Option<(i32, i32)> {
    parse_named_int_cmp(tokens, "radius")
}

/// `velx` / `vely` / `velz` op N (map units) → (axis, cmp, value).
fn parse_vel_threshold(tokens: &[CondAtom]) -> Option<(u8, i32, i32)> {
    for (axis, name) in [(0u8, "velx"), (1u8, "vely"), (2u8, "velz")] {
        if let Some((cmp, value)) = parse_named_int_cmp(tokens, name) {
            return Some((axis, cmp, value));
        }
    }
    None
}

fn parse_named_int_cmp(tokens: &[CondAtom], ident: &str) -> Option<(i32, i32)> {
    if tokens.len() != 3 {
        return None;
    }
    let CondAtom::Ident(lhs) = &tokens[0] else {
        return None;
    };
    if !Scanner::compare(lhs, ident) {
        return None;
    }
    let CondAtom::Int(n) = &tokens[2] else {
        return None;
    };
    let value = i32::try_from(*n).ok()?;
    let cmp = match &tokens[1] {
        CondAtom::Punct(Punct::Eq | Punct::EqEq) => 0,
        CondAtom::Punct(Punct::NotEq) => 1,
        CondAtom::Punct(Punct::Gt) => 2,
        CondAtom::Punct(Punct::GtEq) => 3,
        CondAtom::Punct(Punct::Lt) => 4,
        CondAtom::Punct(Punct::LtEq) => 5,
        _ => return None,
    };
    Some((cmp, value))
}

/// `ident == "str"` / `!=` — cmp 0/1 only.
fn parse_named_string_eq(tokens: &[CondAtom], ident: &str) -> Option<(i32, String)> {
    if tokens.len() != 3 {
        return None;
    }
    let CondAtom::Ident(lhs) = &tokens[0] else {
        return None;
    };
    if !Scanner::compare(lhs, ident) {
        return None;
    }
    let CondAtom::String(s) = &tokens[2] else {
        return None;
    };
    let cmp = match &tokens[1] {
        CondAtom::Punct(Punct::Eq | Punct::EqEq) => 0,
        CondAtom::Punct(Punct::NotEq) => 1,
        _ => return None,
    };
    Some((cmp, s.clone()))
}

fn flatten_string_skip_if(
    action: &str,
    cmp: i32,
    value: &str,
    then_body: Vec<(String, Vec<String>)>,
    else_body: Vec<(String, Vec<String>)>,
) -> Vec<(String, Vec<String>)> {
    let skip = |count: usize| {
        (
            action.to_owned(),
            vec![cmp.to_string(), format!("\"{value}\""), count.to_string()],
        )
    };
    if else_body.is_empty() && !then_body.is_empty() {
        let mut out = vec![skip(then_body.len())];
        out.extend(then_body);
        return out;
    }
    if !else_body.is_empty() {
        let else_len = else_body.len();
        let mut out = vec![skip(then_body.len() + 1)];
        out.extend(then_body);
        out.push(("A_SkipAlways".into(), vec![else_len.to_string()]));
        out.extend(else_body);
        return out;
    }
    Vec::new()
}

/// `ident op N` with int or float RHS, value stored as `round(N * scale)`.
fn parse_named_scaled_cmp(tokens: &[CondAtom], ident: &str, scale: f64) -> Option<(i32, i32)> {
    if tokens.len() != 3 {
        return None;
    }
    let CondAtom::Ident(lhs) = &tokens[0] else {
        return None;
    };
    if !Scanner::compare(lhs, ident) {
        return None;
    }
    let raw = match &tokens[2] {
        CondAtom::Int(n) => i32::try_from(*n).ok()? as f64,
        CondAtom::Float(f) => *f,
        _ => return None,
    };
    let value = (raw * scale).round() as i32;
    let cmp = match &tokens[1] {
        CondAtom::Punct(Punct::Eq | Punct::EqEq) => 0,
        CondAtom::Punct(Punct::NotEq) => 1,
        CondAtom::Punct(Punct::Gt) => 2,
        CondAtom::Punct(Punct::GtEq) => 3,
        CondAtom::Punct(Punct::Lt) => 4,
        CondAtom::Punct(Punct::LtEq) => 5,
        _ => return None,
    };
    Some((cmp, value))
}

/// `user_foo` / `user_foo op N` → (name, cmp, value).
fn parse_user_var_threshold(tokens: &[CondAtom]) -> Option<(String, i32, i32)> {
    let CondAtom::Ident(name) = tokens.first()? else {
        return None;
    };
    if !name.to_ascii_lowercase().starts_with("user_") {
        return None;
    }
    let key = name.to_ascii_lowercase();
    if tokens.len() == 1 {
        // truthy: user_foo != 0
        return Some((key, 1, 0));
    }
    if tokens.len() != 3 {
        return None;
    }
    let CondAtom::Int(n) = &tokens[2] else {
        return None;
    };
    let value = i32::try_from(*n).ok()?;
    let cmp = match &tokens[1] {
        CondAtom::Punct(Punct::Eq | Punct::EqEq) => 0,
        CondAtom::Punct(Punct::NotEq) => 1,
        CondAtom::Punct(Punct::Gt) => 2,
        CondAtom::Punct(Punct::GtEq) => 3,
        CondAtom::Punct(Punct::Lt) => 4,
        CondAtom::Punct(Punct::LtEq) => 5,
        _ => return None,
    };
    Some((key, cmp, value))
}

fn flatten_speed_if(
    cmp: i32,
    value: i32,
    then_body: Vec<(String, Vec<String>)>,
    else_body: Vec<(String, Vec<String>)>,
) -> Vec<(String, Vec<String>)> {
    flatten_cmp_skip_if("A_SkipUnlessSpeed", cmp, value, then_body, else_body)
}

fn flatten_angle_if(
    cmp: i32,
    value: i32,
    then_body: Vec<(String, Vec<String>)>,
    else_body: Vec<(String, Vec<String>)>,
) -> Vec<(String, Vec<String>)> {
    flatten_cmp_skip_if("A_SkipUnlessAngle", cmp, value, then_body, else_body)
}

fn flatten_pitch_if(
    cmp: i32,
    value: i32,
    then_body: Vec<(String, Vec<String>)>,
    else_body: Vec<(String, Vec<String>)>,
) -> Vec<(String, Vec<String>)> {
    flatten_cmp_skip_if("A_SkipUnlessPitch", cmp, value, then_body, else_body)
}

fn flatten_tid_if(
    cmp: i32,
    value: i32,
    then_body: Vec<(String, Vec<String>)>,
    else_body: Vec<(String, Vec<String>)>,
) -> Vec<(String, Vec<String>)> {
    flatten_cmp_skip_if("A_SkipUnlessTid", cmp, value, then_body, else_body)
}

fn flatten_mass_if(
    cmp: i32,
    value: i32,
    then_body: Vec<(String, Vec<String>)>,
    else_body: Vec<(String, Vec<String>)>,
) -> Vec<(String, Vec<String>)> {
    flatten_cmp_skip_if("A_SkipUnlessMass", cmp, value, then_body, else_body)
}

fn flatten_height_if(
    cmp: i32,
    value: i32,
    then_body: Vec<(String, Vec<String>)>,
    else_body: Vec<(String, Vec<String>)>,
) -> Vec<(String, Vec<String>)> {
    flatten_cmp_skip_if("A_SkipUnlessHeight", cmp, value, then_body, else_body)
}

fn flatten_radius_if(
    cmp: i32,
    value: i32,
    then_body: Vec<(String, Vec<String>)>,
    else_body: Vec<(String, Vec<String>)>,
) -> Vec<(String, Vec<String>)> {
    flatten_cmp_skip_if("A_SkipUnlessRadius", cmp, value, then_body, else_body)
}

fn flatten_vel_if(
    axis: u8,
    cmp: i32,
    value: i32,
    then_body: Vec<(String, Vec<String>)>,
    else_body: Vec<(String, Vec<String>)>,
) -> Vec<(String, Vec<String>)> {
    let action = match axis {
        0 => "A_SkipUnlessVelX",
        1 => "A_SkipUnlessVelY",
        _ => "A_SkipUnlessVelZ",
    };
    flatten_cmp_skip_if(action, cmp, value, then_body, else_body)
}

fn flatten_cmp_skip_if(
    action: &str,
    cmp: i32,
    value: i32,
    then_body: Vec<(String, Vec<String>)>,
    else_body: Vec<(String, Vec<String>)>,
) -> Vec<(String, Vec<String>)> {
    let skip = |count: usize| {
        (
            action.to_owned(),
            vec![cmp.to_string(), value.to_string(), count.to_string()],
        )
    };
    if else_body.is_empty() && !then_body.is_empty() {
        let mut out = vec![skip(then_body.len())];
        out.extend(then_body);
        return out;
    }
    if !else_body.is_empty() {
        let else_len = else_body.len();
        let mut out = vec![skip(then_body.len() + 1)];
        out.extend(then_body);
        out.push(("A_SkipAlways".into(), vec![else_len.to_string()]));
        out.extend(else_body);
        return out;
    }
    Vec::new()
}

fn flatten_user_var_if(
    name: &str,
    cmp: i32,
    value: i32,
    then_body: Vec<(String, Vec<String>)>,
    else_body: Vec<(String, Vec<String>)>,
) -> Vec<(String, Vec<String>)> {
    let skip = |count: usize| {
        (
            "A_SkipUnlessUserVar".into(),
            vec![
                name.to_owned(),
                cmp.to_string(),
                value.to_string(),
                count.to_string(),
            ],
        )
    };
    if else_body.is_empty() && !then_body.is_empty() {
        let mut out = vec![skip(then_body.len())];
        out.extend(then_body);
        return out;
    }
    if !else_body.is_empty() {
        let else_len = else_body.len();
        let mut out = vec![skip(then_body.len() + 1)];
        out.extend(then_body);
        out.push(("A_SkipAlways".into(), vec![else_len.to_string()]));
        out.extend(else_body);
        return out;
    }
    Vec::new()
}

fn parse_inventory_threshold(tokens: &[CondAtom]) -> Option<(String, i32)> {
    // CountInv ( "Item" ) [op N]
    if tokens.len() < 4 {
        return None;
    }
    let CondAtom::Ident(fn_name) = &tokens[0] else {
        return None;
    };
    if !(Scanner::compare(fn_name, "CountInv")
        || Scanner::compare(fn_name, "inventory")
        || Scanner::compare(fn_name, "CheckInventory"))
    {
        return None;
    }
    if !matches!(&tokens[1], CondAtom::Punct(Punct::LParen)) {
        return None;
    }
    let CondAtom::String(item) = &tokens[2] else {
        return None;
    };
    if !matches!(&tokens[3], CondAtom::Punct(Punct::RParen)) {
        return None;
    }
    if tokens.len() == 4 {
        return Some((item.clone(), 1));
    }
    if tokens.len() != 6 {
        return None;
    }
    let op = &tokens[4];
    let CondAtom::Int(n) = &tokens[5] else {
        return None;
    };
    let n = i32::try_from(*n).ok()?;
    let amount = match op {
        CondAtom::Punct(Punct::Gt) => n.saturating_add(1),
        CondAtom::Punct(Punct::GtEq) => n.max(1),
        CondAtom::Punct(Punct::Eq) | CondAtom::Punct(Punct::EqEq) => n.max(1),
        _ => return None,
    };
    Some((item.clone(), amount))
}

fn skip_until_semi(sc: &mut Scanner<'_>) -> Result<(), DecorateError> {
    loop {
        match sc.next_token()? {
            Tok::Punct(Punct::Semi) | Tok::Eof => return Ok(()),
            _ => {}
        }
    }
}

/// Read `A_Foo(` … `)` args (opening `(` already consumed).
fn read_action_args(sc: &mut Scanner<'_>) -> Result<Vec<String>, DecorateError> {
    let mut args = Vec::new();
    if sc.check_punct(Punct::RParen)? {
        return Ok(args);
    }
    loop {
        let mut tok = String::new();
        let mut depth = 0i32;
        loop {
            match sc.next_token()? {
                Tok::Punct(Punct::RParen) if depth == 0 => {
                    if !tok.is_empty() {
                        args.push(tok.trim().to_owned());
                    }
                    return Ok(args);
                }
                Tok::Punct(Punct::Comma) if depth == 0 => {
                    args.push(tok.trim().to_owned());
                    break;
                }
                Tok::Punct(Punct::LParen) => {
                    depth += 1;
                    tok.push('(');
                }
                Tok::Punct(Punct::RParen) => {
                    depth -= 1;
                    tok.push(')');
                }
                Tok::Punct(Punct::Minus) => tok.push('-'),
                Tok::Punct(Punct::Plus) => tok.push('+'),
                Tok::Punct(Punct::Dot) => tok.push('.'),
                Tok::Ident(s) => tok.push_str(s),
                Tok::String(s) => {
                    tok.push('"');
                    tok.push_str(&s);
                    tok.push('"');
                }
                Tok::Int(n) => tok.push_str(&n.to_string()),
                Tok::Float(f) => tok.push_str(&f.to_string()),
                Tok::Eof => {
                    return Err(DecorateError::Message(
                        "unclosed action argument list".into(),
                    ));
                }
                other => {
                    return Err(DecorateError::Message(format!(
                        "unexpected in action args: {other:?}"
                    )));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn parses_zombieman() {
        let src = r#"
actor ZombieMan : Actor 3004
{
  Health 20
  Radius 20
  Height 56
  +SOLID
  +SHOOTABLE
  States
  {
  Spawn:
    POSS AB 10 A_Look
    Loop
  See:
    POSS AABBCCDD 4 A_Chase
    Loop
  Death:
    POSS H 5
    POSS I 5 A_Scream
    POSS J 5 A_NoBlocking
    POSS K 5
    POSS L -1
    Stop
  }
}
"#;
        let ir = parse_decorate(src).unwrap();
        assert_eq!(ir.actors.len(), 1);
        let a = &ir.actors[0];
        assert_eq!(a.name, "ZombieMan");
        assert_eq!(a.parent.as_deref(), Some("Actor"));
        assert_eq!(a.doomednum, Some(3004));
        assert!(a.flags.iter().any(|f| f.set && f.name == "SOLID"));
        assert!(a.properties.iter().any(|p| p.name == "Health"));
        let st = a.states.as_ref().unwrap();
        assert!(
            st.items
                .iter()
                .any(|i| matches!(i, StateItem::Label(l) if l == "Spawn"))
        );
        assert!(
            st.items
                .iter()
                .any(|i| matches!(i, StateItem::Flow(StateFlow::Loop)))
        );
    }

    #[test]
    fn parses_bundled_doom_basic() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = parse_decorate(src).unwrap();
        assert!(ir.actors.iter().any(|a| a.name == "ZombieMan"));
        assert!(ir.actors.iter().any(|a| a.name == "Clip"));
        assert!(ir.actors.iter().any(|a| a.name == "DoomPlayer"));
    }

    #[test]
    fn parses_duration_expr_and_anon_function() {
        let src = r#"
actor ExprGuy
{
  States
  {
  Spawn:
    POSS A (4+6) A_Look
    POSS B random(3, 8) { A_Chase; A_FaceTarget }
    Stop
  }
}
"#;
        let ir = parse_decorate(src).unwrap();
        let st = ir.actors[0].states.as_ref().unwrap();
        let lines: Vec<&StateLine> = st
            .items
            .iter()
            .filter_map(|i| match i {
                StateItem::Line(l) => Some(l),
                _ => None,
            })
            .collect();
        assert_eq!(lines[0].duration, 10);
        assert_eq!(lines[0].action.as_deref(), Some("A_Look"));
        assert_eq!(lines[1].duration, 3);
        assert!(lines[1].modifiers.iter().any(|m| m.starts_with("random(")));
        assert_eq!(lines[1].action.as_deref(), Some("A_Chase"));
        assert_eq!(lines[2].duration, 0);
        assert_eq!(lines[2].action.as_deref(), Some("A_FaceTarget"));
    }

    #[test]
    fn parses_decoblock_if_return_state() {
        let src = r#"
actor HurtGuy
{
  States
  {
  Spawn:
    POSS A 1
    {
      if (health < 50)
      {
        return state("Pain");
      }
      A_Look;
    }
    Stop
  Pain:
    POSS H 1
    Stop
  }
}
"#;
        let ir = parse_decorate(src).unwrap();
        let st = ir.actors[0].states.as_ref().unwrap();
        let lines: Vec<&StateLine> = st
            .items
            .iter()
            .filter_map(|i| match i {
                StateItem::Line(l) => Some(l),
                _ => None,
            })
            .collect();
        // First line: JumpIfHealthLower(50, Pain); follow-up A_Look at 0 tics.
        assert_eq!(lines[0].action.as_deref(), Some("A_JumpIfHealthLower"));
        assert_eq!(
            lines[0].action_args,
            vec!["50".to_owned(), "Pain".to_owned()]
        );
        assert_eq!(lines[1].action.as_deref(), Some("A_Look"));
    }

    #[test]
    fn parses_decoblock_health_if_body() {
        let src = r#"
actor BodyIf
{
  States
  {
  Spawn:
    POSS A 1
    {
      if (health < 40)
      {
        A_SetSpeed(20);
        A_Look;
      }
    }
    Stop
  }
}
"#;
        let ir = parse_decorate(src).unwrap();
        let st = ir.actors[0].states.as_ref().unwrap();
        let lines: Vec<&StateLine> = st
            .items
            .iter()
            .filter_map(|i| match i {
                StateItem::Line(l) => Some(l),
                _ => None,
            })
            .collect();
        assert_eq!(lines[0].action.as_deref(), Some("A_SkipUnlessHealthLower"));
        assert_eq!(lines[0].action_args, vec!["40".to_owned(), "2".to_owned()]);
        assert_eq!(lines[1].action.as_deref(), Some("A_SetSpeed"));
        assert_eq!(lines[2].action.as_deref(), Some("A_Look"));
    }

    #[test]
    fn parses_decoblock_args_if_body() {
        let src = r#"
actor ArgsIf
{
  States
  {
  Spawn:
    POSS A 1
    {
      if (args[0] == 3)
      {
        A_Look;
      }
    }
    Stop
  }
}
"#;
        let ir = parse_decorate(src).unwrap();
        let st = ir.actors[0].states.as_ref().unwrap();
        let lines: Vec<&StateLine> = st
            .items
            .iter()
            .filter_map(|i| match i {
                StateItem::Line(l) => Some(l),
                _ => None,
            })
            .collect();
        assert_eq!(lines[0].action.as_deref(), Some("A_SkipUnlessArg"));
        assert_eq!(
            lines[0].action_args,
            vec![
                "0".to_owned(),
                "0".to_owned(),
                "3".to_owned(),
                "1".to_owned()
            ]
        );
        assert_eq!(lines[1].action.as_deref(), Some("A_Look"));
    }

    #[test]
    fn parses_decoblock_inventory_if_body() {
        let src = r#"
actor InvIf
{
  States
  {
  Spawn:
    POSS A 1
    {
      if (CountInv("Clip") >= 5)
      {
        A_Look;
      }
    }
    Stop
  }
}
"#;
        let ir = parse_decorate(src).unwrap();
        let st = ir.actors[0].states.as_ref().unwrap();
        let lines: Vec<&StateLine> = st
            .items
            .iter()
            .filter_map(|i| match i {
                StateItem::Line(l) => Some(l),
                _ => None,
            })
            .collect();
        assert_eq!(lines[0].action.as_deref(), Some("A_SkipUnlessInventory"));
        assert_eq!(
            lines[0].action_args,
            vec!["\"Clip\"".to_owned(), "5".to_owned(), "1".to_owned()]
        );
        assert_eq!(lines[1].action.as_deref(), Some("A_Look"));
    }

    #[test]
    fn parses_decoblock_health_if_else() {
        let src = r#"
actor IfElse
{
  States
  {
  Spawn:
    POSS A 1
    {
      if (health < 30)
      {
        A_SetSpeed(20);
      }
      else
      {
        A_Look;
        A_SetHealth(100);
      }
    }
    Stop
  }
}
"#;
        let ir = parse_decorate(src).unwrap();
        let st = ir.actors[0].states.as_ref().unwrap();
        let lines: Vec<&StateLine> = st
            .items
            .iter()
            .filter_map(|i| match i {
                StateItem::Line(l) => Some(l),
                _ => None,
            })
            .collect();
        assert_eq!(lines[0].action.as_deref(), Some("A_SkipUnlessHealthLower"));
        assert_eq!(lines[0].action_args, vec!["30".to_owned(), "2".to_owned()]);
        assert_eq!(lines[1].action.as_deref(), Some("A_SetSpeed"));
        assert_eq!(lines[2].action.as_deref(), Some("A_SkipAlways"));
        assert_eq!(lines[2].action_args, vec!["2".to_owned()]);
        assert_eq!(lines[3].action.as_deref(), Some("A_Look"));
        assert_eq!(lines[4].action.as_deref(), Some("A_SetHealth"));
    }

    #[test]
    fn parses_decoblock_const_if_else() {
        let src = r#"
actor ConstIf
{
  States
  {
  Spawn:
    POSS A 1
    {
      if (0) { A_Die; }
      else { A_Look; }
    }
    Stop
  }
}
"#;
        let ir = parse_decorate(src).unwrap();
        let st = ir.actors[0].states.as_ref().unwrap();
        let lines: Vec<&StateLine> = st
            .items
            .iter()
            .filter_map(|i| match i {
                StateItem::Line(l) => Some(l),
                _ => None,
            })
            .collect();
        assert_eq!(lines[0].action.as_deref(), Some("A_Look"));
    }

    #[test]
    fn parses_decoblock_speed_and_user_if() {
        let src = r#"
actor SpeedUserIf
{
  States
  {
  Spawn:
    POSS A 1
    {
      if (speed > 10) { A_Look; }
      if (user_mode == 2) { A_SetSpeed(5); }
      if (angle == 90) { A_SetAngle(0); }
      if (pitch < 0) { A_SetPitch(0); }
      if (tid == 7) { A_SetTid(0); }
      if (mass >= 100) { A_SetMass(50); }
      if (height > 56) { A_Look; }
      if (radius < 20) { A_Look; }
      if (velz < 0) { A_Look; }
      if (alpha < 1.0) { A_FadeIn(0.1); }
      if (gravity == 0) { A_SetGravity(1); }
      if (roll == 45) { A_SetRoll(0); }
      if (damage > 0) { A_SetDamage(0); }
      if (score >= 100) { A_SetScore(0); }
      if (special == 80) { A_SetSpecial(0); }
      if (friction < 1.0) { A_SetFriction(1.0); }
      if (damagefactor < 1.0) { A_SetDamageFactor(1.0); }
      if (accuracy >= 10) { A_SetAccuracy(0); }
      if (stamina >= 10) { A_SetStamina(0); }
      if (team == 1) { A_SetTeam(255); }
      if (meleerange > 64) { A_SetMeleeRange(64); }
      if (jumpz < 8) { A_SetJumpZ(8); }
      if (viewheight == 41) { A_SetViewHeight(56); }
      if (attackzoffset != 8) { A_SetAttackZOffset(8); }
      if (maxstepheight > 24) { A_SetMaxStepHeight(24); }
      if (maxdropoffheight < 32) { A_SetMaxDropOffHeight(32); }
      if (floatspeed > 4) { A_SetFloatSpeed(4); }
      if (fastspeed >= 20) { A_SetFastSpeed(15); }
      if (cameraheight == 0) { A_SetCameraHeight(41); }
      if (camerafov != 90) { A_SetCameraFOV(90); }
      if (species == "Pack") { A_SetFriendly(1); }
      if (reactiontime == 0) { A_Look; }
      if (painthreshold > 0) { A_SetPainThreshold(0); }
      if (threshold > 0) { A_SetThreshold(0); }
      if (health >= 50) { A_Look; }
      if (health == 100) { A_Pain; }
    }
    Stop
  }
}
"#;
        let ir = parse_decorate(src).unwrap();
        let st = ir.actors[0].states.as_ref().unwrap();
        let lines: Vec<&StateLine> = st
            .items
            .iter()
            .filter_map(|i| match i {
                StateItem::Line(l) => Some(l),
                _ => None,
            })
            .collect();
        assert_eq!(lines[0].action.as_deref(), Some("A_SkipUnlessSpeed"));
        assert_eq!(
            lines[0].action_args,
            vec!["2".to_owned(), "10".to_owned(), "1".to_owned()]
        );
        assert_eq!(lines[1].action.as_deref(), Some("A_Look"));
        assert_eq!(lines[2].action.as_deref(), Some("A_SkipUnlessUserVar"));
        assert_eq!(
            lines[2].action_args,
            vec![
                "user_mode".to_owned(),
                "0".to_owned(),
                "2".to_owned(),
                "1".to_owned()
            ]
        );
        assert_eq!(lines[3].action.as_deref(), Some("A_SetSpeed"));
        assert_eq!(lines[4].action.as_deref(), Some("A_SkipUnlessAngle"));
        assert_eq!(
            lines[4].action_args,
            vec!["0".to_owned(), "90".to_owned(), "1".to_owned()]
        );
        assert_eq!(lines[5].action.as_deref(), Some("A_SetAngle"));
        assert_eq!(lines[6].action.as_deref(), Some("A_SkipUnlessPitch"));
        assert_eq!(
            lines[6].action_args,
            vec!["4".to_owned(), "0".to_owned(), "1".to_owned()]
        );
        assert_eq!(lines[7].action.as_deref(), Some("A_SetPitch"));
        assert_eq!(lines[8].action.as_deref(), Some("A_SkipUnlessTid"));
        assert_eq!(
            lines[8].action_args,
            vec!["0".to_owned(), "7".to_owned(), "1".to_owned()]
        );
        assert_eq!(lines[9].action.as_deref(), Some("A_SetTid"));
        assert_eq!(lines[10].action.as_deref(), Some("A_SkipUnlessMass"));
        assert_eq!(
            lines[10].action_args,
            vec!["3".to_owned(), "100".to_owned(), "1".to_owned()]
        );
        assert_eq!(lines[11].action.as_deref(), Some("A_SetMass"));
        assert_eq!(lines[12].action.as_deref(), Some("A_SkipUnlessHeight"));
        assert_eq!(
            lines[12].action_args,
            vec!["2".to_owned(), "56".to_owned(), "1".to_owned()]
        );
        assert_eq!(lines[13].action.as_deref(), Some("A_Look"));
        assert_eq!(lines[14].action.as_deref(), Some("A_SkipUnlessRadius"));
        assert_eq!(
            lines[14].action_args,
            vec!["4".to_owned(), "20".to_owned(), "1".to_owned()]
        );
        assert_eq!(lines[15].action.as_deref(), Some("A_Look"));
        assert_eq!(lines[16].action.as_deref(), Some("A_SkipUnlessVelZ"));
        assert_eq!(
            lines[16].action_args,
            vec!["4".to_owned(), "0".to_owned(), "1".to_owned()]
        );
        assert_eq!(lines[17].action.as_deref(), Some("A_Look"));
        assert_eq!(lines[18].action.as_deref(), Some("A_SkipUnlessAlpha"));
        assert_eq!(
            lines[18].action_args,
            vec!["4".to_owned(), "1000".to_owned(), "1".to_owned()]
        );
        assert_eq!(lines[19].action.as_deref(), Some("A_FadeIn"));
        assert_eq!(lines[20].action.as_deref(), Some("A_SkipUnlessGravity"));
        assert_eq!(
            lines[20].action_args,
            vec!["0".to_owned(), "0".to_owned(), "1".to_owned()]
        );
        assert_eq!(lines[21].action.as_deref(), Some("A_SetGravity"));
        assert_eq!(lines[22].action.as_deref(), Some("A_SkipUnlessRoll"));
        assert_eq!(
            lines[22].action_args,
            vec!["0".to_owned(), "45".to_owned(), "1".to_owned()]
        );
        assert_eq!(lines[23].action.as_deref(), Some("A_SetRoll"));
        assert_eq!(lines[24].action.as_deref(), Some("A_SkipUnlessDamage"));
        assert_eq!(
            lines[24].action_args,
            vec!["2".to_owned(), "0".to_owned(), "1".to_owned()]
        );
        assert_eq!(lines[25].action.as_deref(), Some("A_SetDamage"));
        assert_eq!(lines[26].action.as_deref(), Some("A_SkipUnlessScore"));
        assert_eq!(
            lines[26].action_args,
            vec!["3".to_owned(), "100".to_owned(), "1".to_owned()]
        );
        assert_eq!(lines[27].action.as_deref(), Some("A_SetScore"));
        assert_eq!(lines[28].action.as_deref(), Some("A_SkipUnlessSpecial"));
        assert_eq!(
            lines[28].action_args,
            vec!["0".to_owned(), "80".to_owned(), "1".to_owned()]
        );
        assert_eq!(lines[29].action.as_deref(), Some("A_SetSpecial"));
        assert_eq!(lines[30].action.as_deref(), Some("A_SkipUnlessFriction"));
        assert_eq!(
            lines[30].action_args,
            vec!["4".to_owned(), "1000".to_owned(), "1".to_owned()]
        );
        assert_eq!(lines[31].action.as_deref(), Some("A_SetFriction"));
        assert_eq!(
            lines[32].action.as_deref(),
            Some("A_SkipUnlessDamageFactor")
        );
        assert_eq!(
            lines[32].action_args,
            vec!["4".to_owned(), "1000".to_owned(), "1".to_owned()]
        );
        assert_eq!(lines[33].action.as_deref(), Some("A_SetDamageFactor"));
        assert_eq!(lines[34].action.as_deref(), Some("A_SkipUnlessAccuracy"));
        assert_eq!(
            lines[34].action_args,
            vec!["3".to_owned(), "10".to_owned(), "1".to_owned()]
        );
        assert_eq!(lines[35].action.as_deref(), Some("A_SetAccuracy"));
        assert_eq!(lines[36].action.as_deref(), Some("A_SkipUnlessStamina"));
        assert_eq!(
            lines[36].action_args,
            vec!["3".to_owned(), "10".to_owned(), "1".to_owned()]
        );
        assert_eq!(lines[37].action.as_deref(), Some("A_SetStamina"));
        assert_eq!(lines[38].action.as_deref(), Some("A_SkipUnlessTeam"));
        assert_eq!(
            lines[38].action_args,
            vec!["0".to_owned(), "1".to_owned(), "1".to_owned()]
        );
        assert_eq!(lines[39].action.as_deref(), Some("A_SetTeam"));
        assert_eq!(lines[40].action.as_deref(), Some("A_SkipUnlessMeleeRange"));
        assert_eq!(
            lines[40].action_args,
            vec!["2".to_owned(), "64".to_owned(), "1".to_owned()]
        );
        assert_eq!(lines[41].action.as_deref(), Some("A_SetMeleeRange"));
        assert_eq!(lines[42].action.as_deref(), Some("A_SkipUnlessJumpZ"));
        assert_eq!(
            lines[42].action_args,
            vec!["4".to_owned(), "8".to_owned(), "1".to_owned()]
        );
        assert_eq!(lines[43].action.as_deref(), Some("A_SetJumpZ"));
        assert_eq!(lines[44].action.as_deref(), Some("A_SkipUnlessViewHeight"));
        assert_eq!(
            lines[44].action_args,
            vec!["0".to_owned(), "41".to_owned(), "1".to_owned()]
        );
        assert_eq!(lines[45].action.as_deref(), Some("A_SetViewHeight"));
        assert_eq!(
            lines[46].action.as_deref(),
            Some("A_SkipUnlessAttackZOffset")
        );
        assert_eq!(
            lines[46].action_args,
            vec!["1".to_owned(), "8".to_owned(), "1".to_owned()]
        );
        assert_eq!(lines[47].action.as_deref(), Some("A_SetAttackZOffset"));
        assert_eq!(
            lines[48].action.as_deref(),
            Some("A_SkipUnlessMaxStepHeight")
        );
        assert_eq!(
            lines[48].action_args,
            vec!["2".to_owned(), "24".to_owned(), "1".to_owned()]
        );
        assert_eq!(lines[49].action.as_deref(), Some("A_SetMaxStepHeight"));
        assert_eq!(
            lines[50].action.as_deref(),
            Some("A_SkipUnlessMaxDropOffHeight")
        );
        assert_eq!(
            lines[50].action_args,
            vec!["4".to_owned(), "32".to_owned(), "1".to_owned()]
        );
        assert_eq!(lines[51].action.as_deref(), Some("A_SetMaxDropOffHeight"));
        assert_eq!(lines[52].action.as_deref(), Some("A_SkipUnlessFloatSpeed"));
        assert_eq!(
            lines[52].action_args,
            vec!["2".to_owned(), "4".to_owned(), "1".to_owned()]
        );
        assert_eq!(lines[53].action.as_deref(), Some("A_SetFloatSpeed"));
        assert_eq!(lines[54].action.as_deref(), Some("A_SkipUnlessFastSpeed"));
        assert_eq!(
            lines[54].action_args,
            vec!["3".to_owned(), "20".to_owned(), "1".to_owned()]
        );
        assert_eq!(lines[55].action.as_deref(), Some("A_SetFastSpeed"));
        assert_eq!(
            lines[56].action.as_deref(),
            Some("A_SkipUnlessCameraHeight")
        );
        assert_eq!(
            lines[56].action_args,
            vec!["0".to_owned(), "0".to_owned(), "1".to_owned()]
        );
        assert_eq!(lines[57].action.as_deref(), Some("A_SetCameraHeight"));
        assert_eq!(lines[58].action.as_deref(), Some("A_SkipUnlessCameraFOV"));
        assert_eq!(
            lines[58].action_args,
            vec!["1".to_owned(), "90".to_owned(), "1".to_owned()]
        );
        assert_eq!(lines[59].action.as_deref(), Some("A_SetCameraFOV"));
        assert_eq!(lines[60].action.as_deref(), Some("A_SkipUnlessSpecies"));
        assert_eq!(
            lines[60].action_args,
            vec!["0".to_owned(), "\"Pack\"".to_owned(), "1".to_owned()]
        );
        assert_eq!(lines[61].action.as_deref(), Some("A_SetFriendly"));
        assert_eq!(
            lines[62].action.as_deref(),
            Some("A_SkipUnlessReactionTime")
        );
        assert_eq!(
            lines[62].action_args,
            vec!["0".to_owned(), "0".to_owned(), "1".to_owned()]
        );
        assert_eq!(lines[63].action.as_deref(), Some("A_Look"));
        assert_eq!(
            lines[64].action.as_deref(),
            Some("A_SkipUnlessPainThreshold")
        );
        assert_eq!(
            lines[64].action_args,
            vec!["2".to_owned(), "0".to_owned(), "1".to_owned()]
        );
        assert_eq!(lines[65].action.as_deref(), Some("A_SetPainThreshold"));
        assert_eq!(lines[66].action.as_deref(), Some("A_SkipUnlessThreshold"));
        assert_eq!(
            lines[66].action_args,
            vec!["2".to_owned(), "0".to_owned(), "1".to_owned()]
        );
        assert_eq!(lines[67].action.as_deref(), Some("A_SetThreshold"));
        assert_eq!(lines[68].action.as_deref(), Some("A_SkipUnlessHealth"));
        assert_eq!(
            lines[68].action_args,
            vec!["3".to_owned(), "50".to_owned(), "1".to_owned()]
        );
        assert_eq!(lines[69].action.as_deref(), Some("A_Look"));
        assert_eq!(lines[70].action.as_deref(), Some("A_SkipUnlessHealth"));
        assert_eq!(
            lines[70].action_args,
            vec!["0".to_owned(), "100".to_owned(), "1".to_owned()]
        );
        assert_eq!(lines[71].action.as_deref(), Some("A_Pain"));
    }
}
