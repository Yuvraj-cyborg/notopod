//! Running a scene's statements: the mobjects Manim has, made the way
//! Manim makes them, and what `self.add` and `self.play` leave on screen.
//!
//! Only the last frame matters, so an animation is its end state:
//! `Create(c)` puts `c` on screen, `FadeOut(c)` takes it off,
//! `Transform(a, b)` turns `a` into `b`, `c.animate.shift(UP)` moves `c`.
//! Defaults — sizes, colours, gaps — are Manim's own.

use std::collections::HashMap;
use std::f64::consts::{E, PI, TAU};
use std::rc::Rc;

use super::colors;
use super::parse::{Arg, Expr, Stmt};
use super::scene::{
    self, Affine, Arena, Closure, Cubic, Item, Meta, Mob, Rgb, Style, FRAME_H, FRAME_W, P, WHITE,
};
use super::Error;

/// Gaps Manim leaves between things, in units.
const NEXT_TO_BUFF: f64 = 0.25;
const EDGE_BUFF: f64 = 0.5;

/// One em of text at `font_size` 1, in units: Manim's `Text("H")` at 48
/// is 0.441 units tall, and an H is 0.683 em in the fonts used here.
const EM_PER_POINT: f64 = 0.441 / 0.683 / 48.0;

/// Points a plotted function is sampled at.
const SAMPLES: usize = 240;

const RED: Rgb = (0xFC, 0x62, 0x55);
const BLUE: Rgb = (0x58, 0xC4, 0xDD);
const YELLOW: Rgb = (0xF7, 0xD9, 0x6F);
const PURE_YELLOW: Rgb = (0xFF, 0xFF, 0x00);
const BLUE_D: Rgb = (0x29, 0xAB, 0xCA);

#[derive(Debug, Clone)]
pub(crate) enum Value {
    None,
    Bool(bool),
    Num(f64),
    Str(String),
    Point(P),
    Color(Rgb),
    List(Vec<Value>),
    Dict(Vec<(String, Value)>),
    Mob(usize),
    Anim(Anim),
    Lambda(Rc<Closure>),
    /// `np` or `math`.
    Module,
    /// The scene, `self`.
    Scene,
    /// `m.animate` and the methods called on it so far.
    Animate(usize, Vec<(String, Args)>),
    /// Something callable by name: a mobject, an animation, a function.
    Func(String),
}

#[derive(Debug, Clone)]
pub(crate) enum Anim {
    Show(Vec<usize>),
    Hide(Vec<usize>),
    /// `from` takes the shape of `to`.
    Transform {
        from: usize,
        to: usize,
    },
    /// `from` goes, `to` comes.
    Replace {
        from: usize,
        to: usize,
    },
    Methods(usize, Vec<(String, Args)>),
    Target(usize),
    Rotate {
        mob: usize,
        angle: f64,
        about: Option<P>,
    },
    Group(Vec<Anim>),
    Nothing,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Args {
    pos: Vec<Value>,
    key: Vec<(String, Value)>,
}

impl Args {
    fn get(&self, i: usize, key: &str) -> Option<&Value> {
        self.key
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
            .or_else(|| self.pos.get(i))
    }

    fn key(&self, key: &str) -> Option<&Value> {
        self.get(usize::MAX, key)
    }

    /// Refuses keywords other than `allowed` and the style ones, so a
    /// scene is never drawn wrong without saying so.
    fn only(&self, allowed: &[&str]) -> Result<(), Error> {
        const ALWAYS: [&str; 8] = [
            "color",
            "fill_color",
            "fill_opacity",
            "stroke_color",
            "stroke_width",
            "stroke_opacity",
            "z_index",
            "name",
        ];
        match self
            .key
            .iter()
            .find(|(k, _)| !allowed.contains(&k.as_str()) && !ALWAYS.contains(&k.as_str()))
        {
            Some((k, _)) => Err(Error::unsupported(format!("`{k}=`"))),
            None => Ok(()),
        }
    }

    fn num(&self, i: usize, key: &str, default: f64) -> Result<f64, Error> {
        self.get(i, key).map_or(Ok(default), num)
    }

    fn point(&self, i: usize, key: &str, default: P) -> Result<P, Error> {
        match self.get(i, key) {
            None | Some(Value::None) => Ok(default),
            Some(v) => point(v),
        }
    }
}

pub(crate) struct Interp {
    pub arena: Arena,
    /// What is on screen, bottom first.
    pub shown: Vec<usize>,
    env: HashMap<String, Value>,
}

impl Interp {
    pub(crate) fn new() -> Self {
        Self {
            arena: Arena::default(),
            shown: Vec::new(),
            env: HashMap::new(),
        }
    }

    pub(crate) fn run(&mut self, stmts: &[Stmt]) -> Result<(), Error> {
        for stmt in stmts {
            match stmt {
                Stmt::Expr(e) => {
                    self.eval(e)?;
                }
                Stmt::Assign(names, e) => {
                    let value = self.eval(e)?;
                    if let [name] = names.as_slice() {
                        self.env.insert(name.clone(), value);
                    } else {
                        let Value::List(items) = value else {
                            return Err(Error::unsupported("unpacking that"));
                        };
                        if items.len() != names.len() {
                            return Err(Error::unsupported("unpacking that"));
                        }
                        for (name, v) in names.iter().zip(items) {
                            self.env.insert(name.clone(), v);
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn eval(&mut self, e: &Expr) -> Result<Value, Error> {
        Ok(match e {
            Expr::Num(n) => Value::Num(*n),
            Expr::Str(s) => Value::Str(s.clone()),
            Expr::Name(n) => self.lookup(n)?,
            Expr::List(items) => Value::List(
                items
                    .iter()
                    .map(|i| self.eval(i))
                    .collect::<Result<_, _>>()?,
            ),
            Expr::Dict(items) => Value::Dict(
                items
                    .iter()
                    .map(|(k, v)| Ok((k.clone(), self.eval(v)?)))
                    .collect::<Result<_, Error>>()?,
            ),
            Expr::Neg(x) => match self.eval(x)? {
                Value::Num(n) => Value::Num(-n),
                Value::Point(p) => Value::Point([-p[0], -p[1]]),
                _ => return Err(Error::unsupported("negating that")),
            },
            Expr::Bin(op, l, r) => {
                let (l, r) = (self.eval(l)?, self.eval(r)?);
                arith(*op, &l, &r)?
            }
            Expr::Attr(obj, name) => {
                let obj = self.eval(obj)?;
                self.attr(&obj, name)?
            }
            Expr::Call(callee, args) => {
                if let Expr::Attr(obj, method) = callee.as_ref() {
                    let obj = self.eval(obj)?;
                    let args = self.args(args)?;
                    return self.method(obj, method, args);
                }
                let f = self.eval(callee)?;
                let args = self.args(args)?;
                self.call(&f, &args)?
            }
            Expr::Index(obj, i) => {
                let (obj, i) = (self.eval(obj)?, self.eval(i)?);
                index(&self.arena, &obj, &i)?
            }
            Expr::Lambda(params, body) => Value::Lambda(Rc::new(Closure {
                params: params.clone(),
                body: (**body).clone(),
            })),
        })
    }

    fn args(&mut self, args: &[Arg]) -> Result<Args, Error> {
        let mut out = Args::default();
        for a in args {
            match a {
                Arg::Pos(e) => out.pos.push(self.eval(e)?),
                Arg::Star(e) => match self.eval(e)? {
                    Value::List(items) => out.pos.extend(items),
                    Value::Mob(id) => out
                        .pos
                        .extend(self.arena.get(id).children.iter().map(|c| Value::Mob(*c))),
                    _ => return Err(Error::unsupported("unpacking that")),
                },
                Arg::Key(k, e) => {
                    let v = self.eval(e)?;
                    out.key.push((k.clone(), v));
                }
            }
        }
        Ok(out)
    }

    fn lookup(&self, name: &str) -> Result<Value, Error> {
        if let Some(v) = self.env.get(name) {
            return Ok(v.clone());
        }
        let dir = |x: f64, y: f64| Ok(Value::Point([x, y]));
        match name {
            "self" => return Ok(Value::Scene),
            "np" | "numpy" | "math" => return Ok(Value::Module),
            "True" => return Ok(Value::Bool(true)),
            "False" => return Ok(Value::Bool(false)),
            "None" => return Ok(Value::None),
            "ORIGIN" | "OUT" | "IN" => return dir(0.0, 0.0),
            "UP" => return dir(0.0, 1.0),
            "DOWN" => return dir(0.0, -1.0),
            "LEFT" => return dir(-1.0, 0.0),
            "RIGHT" => return dir(1.0, 0.0),
            "UL" => return dir(-1.0, 1.0),
            "UR" => return dir(1.0, 1.0),
            "DL" => return dir(-1.0, -1.0),
            "DR" => return dir(1.0, -1.0),
            "PI" => return Ok(Value::Num(PI)),
            "TAU" => return Ok(Value::Num(TAU)),
            "DEGREES" => return Ok(Value::Num(PI / 180.0)),
            "SMALL_BUFF" => return Ok(Value::Num(0.1)),
            "MED_SMALL_BUFF" | "DEFAULT_MOBJECT_TO_MOBJECT_BUFFER" => {
                return Ok(Value::Num(NEXT_TO_BUFF));
            }
            "MED_LARGE_BUFF" | "DEFAULT_MOBJECT_TO_EDGE_BUFFER" => {
                return Ok(Value::Num(EDGE_BUFF));
            }
            "LARGE_BUFF" => return Ok(Value::Num(1.0)),
            _ => {}
        }
        if let Some(c) = colors::named(name) {
            return Ok(Value::Color(c));
        }
        if CALLABLE.contains(&name) || ANIMS.contains(&name) {
            return Ok(Value::Func(name.to_owned()));
        }
        // Rate functions only change how an animation moves, not where
        // it ends.
        if RATE_FUNCS.contains(&name) || name.starts_with("ease_") {
            return Ok(Value::None);
        }
        Err(Error::unsupported(format!("`{name}`")))
    }

    fn attr(&mut self, obj: &Value, name: &str) -> Result<Value, Error> {
        match obj {
            Value::Mob(id) => {
                let id = *id;
                match name {
                    "animate" => Ok(Value::Animate(id, Vec::new())),
                    "width" => Ok(Value::Num(self.arena.size(id)[0])),
                    "height" => Ok(Value::Num(self.arena.size(id)[1])),
                    "x_axis" | "y_axis" => match &self.arena.get(id).meta {
                        Meta::Axes { x_axis, y_axis, .. } => {
                            Ok(Value::Mob(if name == "x_axis" { *x_axis } else { *y_axis }))
                        }
                        _ => Err(Error::unsupported(format!("`.{name}`"))),
                    },
                    "target" => self
                        .arena
                        .get(id)
                        .target
                        .map(Value::Mob)
                        .ok_or_else(|| Error::unsupported("`.target` before generate_target")),
                    "submobjects" => Ok(Value::List(
                        self.arena
                            .get(id)
                            .children
                            .iter()
                            .map(|c| Value::Mob(*c))
                            .collect(),
                    )),
                    _ => Err(Error::unsupported(format!("`.{name}`"))),
                }
            }
            Value::Module => match name {
                "pi" => Ok(Value::Num(PI)),
                "e" => Ok(Value::Num(E)),
                "tau" => Ok(Value::Num(TAU)),
                f if MATH_FUNCS.contains(&f) || f == "array" => Ok(Value::Func(format!("np.{f}"))),
                _ => Err(Error::unsupported(format!("`np.{name}`"))),
            },
            _ => Err(Error::unsupported(format!("`.{name}`"))),
        }
    }

    fn method(&mut self, obj: Value, name: &str, args: Args) -> Result<Value, Error> {
        match obj {
            Value::Scene => self.scene_method(name, &args),
            Value::Mob(id) => self.mob_method(id, name, &args),
            Value::Animate(id, mut calls) => {
                calls.push((name.to_owned(), args));
                Ok(Value::Animate(id, calls))
            }
            Value::Module => self.call(&Value::Func(format!("np.{name}")), &args),
            _ => Err(Error::unsupported(format!("`.{name}()`"))),
        }
    }

    fn call(&mut self, f: &Value, args: &Args) -> Result<Value, Error> {
        match f {
            Value::Lambda(closure) => self.apply(closure, &args.pos),
            Value::Func(name) => {
                if let Some(f) = name.strip_prefix("np.") {
                    return numpy(f, args);
                }
                if ANIMS.contains(&name.as_str()) {
                    return animation(name, args).map(Value::Anim);
                }
                self.construct(name, args)
            }
            _ => Err(Error::unsupported("calling that")),
        }
    }

    fn apply(&mut self, closure: &Closure, pos: &[Value]) -> Result<Value, Error> {
        if closure.params.len() != pos.len() {
            return Err(Error::unsupported("calling a lambda with other arguments"));
        }
        let saved: Vec<(String, Option<Value>)> = closure
            .params
            .iter()
            .zip(pos)
            .map(|(p, v)| (p.clone(), self.env.insert(p.clone(), v.clone())))
            .collect();
        let result = self.eval(&closure.body);
        for (p, old) in saved {
            match old {
                Some(v) => self.env.insert(p, v),
                None => self.env.remove(&p),
            };
        }
        result
    }

    // ----- the scene -----

    fn scene_method(&mut self, name: &str, args: &Args) -> Result<Value, Error> {
        match name {
            "add" | "add_foreground_mobject" | "add_foreground_mobjects" | "bring_to_front" => {
                for id in mobs(&args.pos)? {
                    self.show(id);
                }
            }
            "remove" => {
                for id in mobs(&args.pos)? {
                    self.hide(id);
                }
            }
            "bring_to_back" => {
                for id in mobs(&args.pos)?.into_iter().rev() {
                    self.shown.retain(|m| *m != id);
                    self.shown.insert(0, id);
                }
            }
            "play" => {
                for a in &args.pos {
                    match a {
                        Value::Anim(anim) => self.play(anim)?,
                        Value::Animate(id, calls) => {
                            self.play(&Anim::Methods(*id, calls.clone()))?;
                        }
                        _ => return Err(Error::unsupported("playing that")),
                    }
                }
            }
            "wait" | "next_section" => {}
            "clear" => self.shown.clear(),
            _ => return Err(Error::unsupported(format!("`self.{name}`"))),
        }
        Ok(Value::None)
    }

    fn show(&mut self, id: usize) {
        self.shown.retain(|m| *m != id);
        self.shown.push(id);
    }

    fn hide(&mut self, id: usize) {
        self.shown.retain(|m| *m != id);
        // Taken out of whatever group it was shown in, too.
        for m in self.shown.clone() {
            for f in self.arena.family(m) {
                self.arena.get_mut(f).children.retain(|c| *c != id);
            }
        }
    }

    fn play(&mut self, anim: &Anim) -> Result<(), Error> {
        match anim {
            Anim::Show(ids) => ids.iter().for_each(|id| {
                if !self.on_screen(*id) {
                    self.show(*id);
                }
            }),
            Anim::Hide(ids) => ids.iter().for_each(|id| self.hide(*id)),
            Anim::Transform { from, to } => {
                self.become_copy(*from, *to)?;
                if !self.on_screen(*from) {
                    self.show(*from);
                }
            }
            Anim::Replace { from, to } => {
                self.hide(*from);
                self.show(*to);
            }
            Anim::Methods(id, calls) => {
                for (name, args) in calls {
                    self.mob_method(*id, name, args)?;
                }
                if !self.on_screen(*id) {
                    self.show(*id);
                }
            }
            Anim::Target(id) => {
                let target = self
                    .arena
                    .get(*id)
                    .target
                    .ok_or_else(|| Error::unsupported("MoveToTarget before generate_target"))?;
                self.become_copy(*id, target)?;
            }
            Anim::Rotate { mob, angle, about } => {
                let about = about.unwrap_or_else(|| self.arena.center(*mob));
                self.arena.transform(*mob, &Affine::rotate(*angle, about))?;
            }
            Anim::Group(anims) => {
                for a in anims {
                    self.play(a)?;
                }
            }
            Anim::Nothing => {}
        }
        Ok(())
    }

    fn on_screen(&self, id: usize) -> bool {
        self.shown
            .iter()
            .any(|m| *m == id || self.arena.family(*m).contains(&id))
    }

    /// `id` takes the look and shape of a copy of `other`, staying itself.
    fn become_copy(&mut self, id: usize, other: usize) -> Result<(), Error> {
        let copy = self.arena.deep_copy(other)?;
        let copied = self.arena.get(copy).clone();
        let mob = self.arena.get_mut(id);
        mob.items = copied.items;
        mob.children = copied.children;
        mob.meta = copied.meta;
        Ok(())
    }

    // ----- making mobjects -----

    fn add(&mut self, mob: Mob) -> Result<Value, Error> {
        self.arena.add(mob).map(Value::Mob)
    }

    fn construct(&mut self, name: &str, args: &Args) -> Result<Value, Error> {
        if let Some(mob) = shape(name, args)?.or(text(name, args)?) {
            return self.add(mob);
        }
        let mob = match name {
            "Cross" => {
                args.only(&["mobject", "scale_factor"])?;
                let (size, mid) = match args.get(0, "mobject") {
                    Some(Value::Mob(id)) => (self.arena.size(*id), self.arena.center(*id)),
                    _ => ([2.0, 2.0], [0.0, 0.0]),
                };
                let scale = args.num(1, "scale_factor", 1.0)?;
                let half = [size[0] * scale / 2.0, size[1] * scale / 2.0];
                let corner = |sx: f64, sy: f64| [mid[0] + sx * half[0], mid[1] + sy * half[1]];
                let subpaths = vec![
                    vec![scene::line(corner(-1.0, 1.0), corner(1.0, -1.0))],
                    vec![scene::line(corner(1.0, 1.0), corner(-1.0, -1.0))],
                ];
                let base = Style {
                    stroke_width: 6.0,
                    ..Style::outline(RED)
                };
                Mob::path(subpaths, false, style(args, base)?)
            }
            "SurroundingRectangle" => {
                args.only(&["buff", "corner_radius"])?;
                let ids = mobs(&args.pos)?;
                let buff = args.num(usize::MAX, "buff", 0.1)?;
                let r = args.num(usize::MAX, "corner_radius", 0.0)?;
                let group = self.arena.add(Mob::group(ids))?;
                let (lo, hi) = self.arena.bounds(group).unwrap_or(([0.0; 2], [0.0; 2]));
                let (w, h) = (hi[0] - lo[0] + 2.0 * buff, hi[1] - lo[1] + 2.0 * buff);
                let mut m = Mob::path(
                    vec![scene::rounded_rect(w, h, r)],
                    true,
                    style(args, Style::outline(PURE_YELLOW))?,
                );
                shift_items(
                    &mut m,
                    [f64::midpoint(lo[0], hi[0]), f64::midpoint(lo[1], hi[1])],
                );
                m
            }
            "VGroup" | "Group" => {
                args.only(&[])?;
                Mob::group(mobs(&args.pos)?)
            }
            "Axes" => return self.axes(args, false),
            "NumberPlane" => return self.axes(args, true),
            "NumberLine" => return self.number_line(args),
            "FunctionGraph" => {
                args.only(&["function", "x_range"])?;
                let Some(Value::Lambda(f)) = args.get(0, "function") else {
                    return Err(Error::unsupported("FunctionGraph without a lambda"));
                };
                let f = Rc::clone(f);
                let range = range(args.key("x_range"), [-FRAME_W / 2.0, FRAME_W / 2.0, 1.0])?;
                return self.graph(&f, Affine::IDENTITY, range, args, YELLOW);
            }
            "ManimColor" => {
                return args
                    .get(0, "value")
                    .map_or(Err(Error::unsupported("ManimColor()")), |v| {
                        color(v).map(Value::Color)
                    });
            }
            "interpolate_color" => {
                let (x, y) = (
                    color(args.get(0, "color1").unwrap_or(&Value::None))?,
                    color(args.get(1, "color2").unwrap_or(&Value::None))?,
                );
                let t = args.num(2, "alpha", 0.5)?.clamp(0.0, 1.0);
                let mix =
                    |p: u8, q: u8| (f64::from(p) + (f64::from(q) - f64::from(p)) * t).round() as u8;
                return Ok(Value::Color((mix(x.0, y.0), mix(x.1, y.1), mix(x.2, y.2))));
            }
            "abs" | "min" | "max" | "round" | "float" | "int" | "len" | "str" => {
                return builtin(name, args);
            }
            _ => return Err(Error::unsupported(format!("`{name}`"))),
        };
        self.add(mob)
    }

    // ----- methods -----

    #[allow(clippy::too_many_lines)]
    fn mob_method(&mut self, id: usize, name: &str, args: &Args) -> Result<Value, Error> {
        let me = Value::Mob(id);
        match name {
            "shift" => {
                let mut by = [0.0, 0.0];
                for v in &args.pos {
                    let p = point(v)?;
                    by = [by[0] + p[0], by[1] + p[1]];
                }
                self.arena.transform(id, &Affine::shift(by))?;
            }
            "move_to" => {
                let edge = args.point(1, "aligned_edge", [0.0, 0.0])?;
                let target = self.target_point(args.get(0, "point_or_mobject"), edge)?;
                let from = self.arena.critical(id, edge);
                self.arena.transform(
                    id,
                    &Affine::shift([target[0] - from[0], target[1] - from[1]]),
                )?;
            }
            "next_to" => {
                let dir = args.point(1, "direction", [1.0, 0.0])?;
                let buff = args.num(2, "buff", NEXT_TO_BUFF)?;
                let edge = args.point(3, "aligned_edge", [0.0, 0.0])?;
                let target = self.target_point(
                    args.get(0, "mobject_or_point"),
                    [edge[0] + dir[0], edge[1] + dir[1]],
                )?;
                let from = self
                    .arena
                    .critical(id, [edge[0] - dir[0], edge[1] - dir[1]]);
                self.arena.transform(
                    id,
                    &Affine::shift([
                        target[0] - from[0] + buff * dir[0],
                        target[1] - from[1] + buff * dir[1],
                    ]),
                )?;
            }
            "to_edge" | "to_corner" => {
                let default = if name == "to_edge" {
                    [-1.0, 0.0]
                } else {
                    [-1.0, -1.0]
                };
                let dir = args.point(
                    0,
                    if name == "to_edge" { "edge" } else { "corner" },
                    default,
                )?;
                let buff = args.num(1, "buff", EDGE_BUFF)?;
                let at = self.arena.critical(id, dir);
                let radius = [FRAME_W / 2.0, FRAME_H / 2.0];
                let mut by = [0.0, 0.0];
                for i in 0..2 {
                    if dir[i] != 0.0 {
                        by[i] = dir[i].signum() * radius[i] - at[i] - buff * dir[i];
                    }
                }
                self.arena.transform(id, &Affine::shift(by))?;
            }
            "align_to" => {
                let dir = args.point(1, "direction", [0.0, 0.0])?;
                let target = self.target_point(args.get(0, "mobject_or_point"), dir)?;
                let at = self.arena.critical(id, dir);
                let by = [
                    if dir[0] == 0.0 {
                        0.0
                    } else {
                        target[0] - at[0]
                    },
                    if dir[1] == 0.0 {
                        0.0
                    } else {
                        target[1] - at[1]
                    },
                ];
                self.arena.transform(id, &Affine::shift(by))?;
            }
            "center" => {
                let c = self.arena.center(id);
                self.arena.transform(id, &Affine::shift([-c[0], -c[1]]))?;
            }
            "set_x" | "set_y" => {
                let i = usize::from(name == "set_y");
                let v = args.num(0, if i == 0 { "x" } else { "y" }, 0.0)?;
                let c = self.arena.center(id);
                let mut by = [0.0, 0.0];
                by[i] = v - c[i];
                self.arena.transform(id, &Affine::shift(by))?;
            }
            "scale" => {
                let k = args.num(0, "scale_factor", 1.0)?;
                let about = self.about(id, args)?;
                self.arena.transform(id, &Affine::scale(k, k, about))?;
            }
            "rotate" => {
                let angle = args.num(0, "angle", 0.0)?;
                if let Some(axis) = args.get(1, "axis") {
                    if point(axis)? != [0.0, 0.0] {
                        return Err(Error::unsupported("turning in 3D"));
                    }
                }
                let about = self.about(id, args)?;
                self.arena.transform(id, &Affine::rotate(angle, about))?;
            }
            "flip" => {
                let c = self.arena.center(id);
                self.arena.transform(id, &Affine::scale(-1.0, 1.0, c))?;
            }
            "stretch" => {
                let k = args.num(0, "factor", 1.0)?;
                let dim = count(args.get(1, "dim"), 0)?;
                let c = self.arena.center(id);
                let t = if dim == 0 {
                    Affine::scale(k, 1.0, c)
                } else {
                    Affine::scale(1.0, k, c)
                };
                self.arena.transform(id, &t)?;
            }
            "set_width" | "scale_to_fit_width" | "set_height" | "scale_to_fit_height" => {
                let i = usize::from(name.ends_with("height"));
                let want = args.num(0, if i == 0 { "width" } else { "height" }, 1.0)?;
                let have = self.arena.size(id)[i];
                if have > 0.0 {
                    let c = self.arena.center(id);
                    let k = want / have;
                    self.arena.transform(id, &Affine::scale(k, k, c))?;
                }
            }
            "set_color" | "set_colour" => {
                let c = color(args.get(0, "color").unwrap_or(&Value::None))?;
                self.arena.each_item(id, |item| set_color(item, c));
            }
            "set_fill" => {
                let c = args
                    .get(0, "color")
                    .filter(|v| !matches!(v, Value::None))
                    .map(color)
                    .transpose()?;
                let o = args
                    .get(1, "opacity")
                    .filter(|v| !matches!(v, Value::None))
                    .map(num)
                    .transpose()?;
                self.arena.each_item(id, |item| match item {
                    Item::Path { style, .. } => {
                        if let Some(c) = c {
                            style.fill = c;
                        }
                        if let Some(o) = o {
                            style.fill_opacity = o;
                        }
                    }
                    Item::Tex { color, opacity, .. } => {
                        if let Some(c) = c {
                            *color = c;
                        }
                        if let Some(o) = o {
                            *opacity = o;
                        }
                    }
                });
            }
            "set_stroke" => {
                let c = args
                    .get(0, "color")
                    .filter(|v| !matches!(v, Value::None))
                    .map(color)
                    .transpose()?;
                let w = args
                    .get(1, "width")
                    .filter(|v| !matches!(v, Value::None))
                    .map(num)
                    .transpose()?;
                let o = args
                    .get(2, "opacity")
                    .filter(|v| !matches!(v, Value::None))
                    .map(num)
                    .transpose()?;
                self.arena.each_item(id, |item| {
                    if let Item::Path { style, .. } = item {
                        if let Some(c) = c {
                            style.stroke = c;
                        }
                        if let Some(w) = w {
                            style.stroke_width = w;
                        }
                        if let Some(o) = o {
                            style.stroke_opacity = o;
                        }
                    }
                });
            }
            "set_opacity" | "fade" => {
                let o = if name == "fade" {
                    1.0 - args.num(0, "darkness", 0.5)?
                } else {
                    args.num(0, "opacity", 1.0)?
                };
                let fade = name == "fade";
                self.arena.each_item(id, |item| match item {
                    Item::Path { style, .. } => {
                        style.fill_opacity = if fade { style.fill_opacity * o } else { o };
                        style.stroke_opacity = if fade { style.stroke_opacity * o } else { o };
                    }
                    Item::Tex { opacity, .. } => *opacity = if fade { *opacity * o } else { o },
                });
            }
            "set_z_index" => {}
            "arrange" => {
                let dir = args.point(0, "direction", [1.0, 0.0])?;
                let buff = args.num(1, "buff", NEXT_TO_BUFF)?;
                let center = !matches!(args.get(2, "center"), Some(Value::Bool(false)));
                let edge = args.point(3, "aligned_edge", [0.0, 0.0])?;
                let children = self.arena.get(id).children.clone();
                for pair in children.windows(2) {
                    let args = positional(vec![
                        Value::Mob(pair[0]),
                        Value::Point(dir),
                        Value::Num(buff),
                        Value::Point(edge),
                    ]);
                    self.mob_method(pair[1], "next_to", &args)?;
                }
                if center {
                    self.mob_method(id, "center", &Args::default())?;
                }
            }
            "add" => {
                let ids = mobs(&args.pos)?;
                self.arena.get_mut(id).children.extend(ids);
            }
            "copy" => return self.arena.deep_copy(id).map(Value::Mob),
            "become" => {
                let Some(Value::Mob(other)) = args.get(0, "mobject") else {
                    return Err(Error::unsupported("become() without a mobject"));
                };
                self.become_copy(id, *other)?;
            }
            "generate_target" => {
                let t = self.arena.deep_copy(id)?;
                self.arena.get_mut(id).target = Some(t);
                return Ok(Value::Mob(t));
            }
            "get_center" => return Ok(Value::Point(self.arena.center(id))),
            "get_top" | "get_bottom" | "get_left" | "get_right" => {
                let dir = match name {
                    "get_top" => [0.0, 1.0],
                    "get_bottom" => [0.0, -1.0],
                    "get_left" => [-1.0, 0.0],
                    _ => [1.0, 0.0],
                };
                return Ok(Value::Point(self.arena.critical(id, dir)));
            }
            "get_corner" | "get_edge_center" | "get_critical_point" | "get_boundary_point" => {
                let dir = args.point(0, "direction", [0.0, 0.0])?;
                return Ok(Value::Point(self.arena.critical(id, dir)));
            }
            "get_x" | "get_y" => {
                return Ok(Value::Num(
                    self.arena.center(id)[usize::from(name == "get_y")],
                ));
            }
            "get_width" => return Ok(Value::Num(self.arena.size(id)[0])),
            "get_height" => return Ok(Value::Num(self.arena.size(id)[1])),
            "get_start" | "get_end" => {
                return match &self.arena.get(id).meta {
                    Meta::Line { start, end } | Meta::Graph { start, end, .. } => {
                        Ok(Value::Point(if name == "get_start" {
                            *start
                        } else {
                            *end
                        }))
                    }
                    _ => Err(Error::unsupported(format!("`.{name}()` here"))),
                };
            }
            _ => return self.axes_method(id, name, args),
        }
        Ok(me)
    }

    /// Where `about_point` or `about_edge` say, or the middle.
    fn about(&self, id: usize, args: &Args) -> Result<P, Error> {
        if let Some(p) = args
            .key("about_point")
            .filter(|v| !matches!(v, Value::None))
        {
            return point(p);
        }
        if let Some(e) = args.key("about_edge").filter(|v| !matches!(v, Value::None)) {
            return Ok(self.arena.critical(id, point(e)?));
        }
        Ok(self.arena.center(id))
    }

    fn target_point(&self, target: Option<&Value>, dir: P) -> Result<P, Error> {
        match target {
            Some(Value::Mob(t)) => Ok(self.arena.critical(*t, dir)),
            Some(v) => point(v),
            None => Ok([0.0, 0.0]),
        }
    }

    // ----- axes -----

    fn axes(&mut self, args: &Args, plane: bool) -> Result<Value, Error> {
        args.only(&[
            "x_range",
            "y_range",
            "x_length",
            "y_length",
            "axis_config",
            "x_axis_config",
            "y_axis_config",
            "tips",
            "background_line_style",
        ])?;
        let default_x = if plane {
            [-FRAME_W / 2.0, FRAME_W / 2.0, 1.0]
        } else {
            [-7.0, 7.0, 1.0]
        };
        let x = range(args.get(0, "x_range"), default_x)?;
        let y = range(args.get(1, "y_range"), [-4.0, 4.0, 1.0])?;
        let (x_len, y_len) = if plane {
            (
                args.num(2, "x_length", x[1] - x[0])?,
                args.num(3, "y_length", y[1] - y[0])?,
            )
        } else {
            (
                args.num(2, "x_length", 12.0)?,
                args.num(3, "y_length", 6.0)?,
            )
        };
        let sx = x_len / (x[1] - x[0]);
        let sy = y_len / (y[1] - y[0]);
        let map = Affine::new(
            sx,
            0.0,
            0.0,
            sy,
            -f64::midpoint(x[0], x[1]) * sx,
            -f64::midpoint(y[0], y[1]) * sy,
        );
        let tips = !plane && !matches!(args.key("tips"), Some(Value::Bool(false)));
        let shared = axis_config(args.key("axis_config"), AxisStyle::new(tips, !plane))?;
        let x_style = axis_config(args.key("x_axis_config"), shared.clone())?;
        let y_style = axis_config(args.key("y_axis_config"), shared)?;
        let cross = [x[0].max(0.0).min(x[1]), y[0].max(0.0).min(y[1])];

        let mut children = Vec::new();
        if plane {
            let mut s = Style::outline(BLUE_D);
            s.stroke_width = 2.0;
            if let Some(Value::Dict(items)) = args.key("background_line_style") {
                for (k, v) in items {
                    match k.as_str() {
                        "stroke_color" | "color" => s.stroke = color(v)?,
                        "stroke_width" => s.stroke_width = num(v)?,
                        "stroke_opacity" => s.stroke_opacity = num(v)?,
                        _ => {
                            return Err(Error::unsupported(format!(
                                "`{k}` in background_line_style"
                            )))
                        }
                    }
                }
            }
            let mut lines = Vec::new();
            for v in steps(x, cross[0]) {
                lines.push(vec![scene::line(
                    map.apply([v, y[0]]),
                    map.apply([v, y[1]]),
                )]);
            }
            for v in steps(y, cross[1]) {
                lines.push(vec![scene::line(
                    map.apply([x[0], v]),
                    map.apply([x[1], v]),
                )]);
            }
            children.push(self.arena.add(Mob::path(lines, false, s))?);
        }
        let x_axis = self.axis(map, x, cross, &x_style, true, true)?;
        let y_axis = self.axis(map, y, cross, &y_style, false, true)?;
        children.extend([x_axis, y_axis]);
        let mut mob = Mob::group(children);
        mob.meta = Meta::Axes {
            x,
            y,
            map,
            x_axis,
            y_axis,
        };
        self.add(mob)
    }

    /// One axis: its line, ticks, tip and numbers.
    fn axis(
        &mut self,
        map: Affine,
        r: [f64; 3],
        cross: P,
        s: &AxisStyle,
        horizontal: bool,
        skip_origin: bool,
    ) -> Result<usize, Error> {
        let at = |v: f64| {
            if horizontal {
                map.apply([v, cross[1]])
            } else {
                map.apply([cross[0], v])
            }
        };
        let cross_value = if horizontal { cross[0] } else { cross[1] };
        let (start, end) = (at(r[0]), at(r[1]));
        let mut items = Vec::new();
        let mut style = Style::outline(s.color);
        style.stroke_width = s.stroke_width;
        let dir = unit([end[0] - start[0], end[1] - start[1]]);
        let line_end = if s.tip {
            [end[0] - dir[0] * TIP, end[1] - dir[1] * TIP]
        } else {
            end
        };
        let mut subpaths = vec![vec![scene::line(start, line_end)]];
        let values: Vec<f64> = steps(r, cross_value)
            .into_iter()
            .filter(|v| (v - cross_value).abs() > 1e-9 || !skip_origin)
            .filter(|v| !s.tip || (v - r[1]).abs() > 1e-9)
            .collect();
        if s.ticks {
            let normal = [-dir[1] * 0.1, dir[0] * 0.1];
            for v in &values {
                let p = at(*v);
                subpaths.push(vec![scene::line(
                    [p[0] - normal[0], p[1] - normal[1]],
                    [p[0] + normal[0], p[1] + normal[1]],
                )]);
            }
        }
        items.push(Item::Path {
            subpaths,
            closed: false,
            style,
            dash: None,
        });
        if s.tip {
            items.push(tip(end, dir, TIP, s.color));
        }
        let mut axis = Mob::new(items);
        if s.numbers {
            let decimals = decimals(r[2]);
            for v in &values {
                let args = Args {
                    pos: Vec::new(),
                    key: vec![("font_size".into(), Value::Num(s.font_size))],
                };
                let label = tex_mob(&format!("{v:.decimals$}"), true, &args, "axis numbers")?;
                let id = self.arena.add(label)?;
                let p = at(*v);
                let side = if horizontal { [0.0, -1.0] } else { [-1.0, 0.0] };
                let next = positional(vec![
                    Value::Point(p),
                    Value::Point(side),
                    Value::Num(NEXT_TO_BUFF),
                ]);
                self.mob_method(id, "next_to", &next)?;
                axis.children.push(id);
            }
        }
        self.arena.add(axis)
    }

    fn number_line(&mut self, args: &Args) -> Result<Value, Error> {
        args.only(&[
            "x_range",
            "length",
            "unit_size",
            "include_numbers",
            "include_tip",
            "include_ticks",
            "font_size",
        ])?;
        let r = range(args.get(0, "x_range"), [-7.0, 7.0, 1.0])?;
        let unit = args.num(usize::MAX, "unit_size", 1.0)?;
        let length = args.num(1, "length", (r[1] - r[0]) * unit)?;
        let s = length / (r[1] - r[0]);
        let map = Affine::new(s, 0.0, 0.0, 1.0, -f64::midpoint(r[0], r[1]) * s, 0.0);
        let mut style = AxisStyle::new(truthy(args.key("include_tip")), true);
        style.numbers = truthy(args.key("include_numbers"));
        style.ticks = !matches!(args.key("include_ticks"), Some(Value::Bool(false)));
        if let Some(c) = args.key("color") {
            style.color = color(c)?;
        }
        if let Some(f) = args.key("font_size") {
            style.font_size = num(f)?;
        }
        let id = self.axis(map, r, [0.0, 0.0], &style, true, false)?;
        self.arena.get_mut(id).meta = Meta::Axes {
            x: r,
            y: [0.0, 0.0, 1.0],
            map,
            x_axis: id,
            y_axis: id,
        };
        Ok(Value::Mob(id))
    }

    fn graph(
        &mut self,
        func: &Rc<Closure>,
        map: Affine,
        span: [f64; 3],
        args: &Args,
        default: Rgb,
    ) -> Result<Value, Error> {
        let mut subpaths: Vec<Vec<Cubic>> = Vec::new();
        let mut run: Vec<P> = Vec::new();
        let mut first = None;
        let mut last = None;
        for i in 0..=SAMPLES {
            let x = span[0] + (span[1] - span[0]) * i as f64 / SAMPLES as f64;
            let y = num(&self.apply(func, &[Value::Num(x)])?)?;
            if y.is_finite() && y.abs() < 1e6 {
                let p = map.apply([x, y]);
                first.get_or_insert(p);
                last = Some(p);
                run.push(p);
            } else if run.len() > 1 {
                subpaths.push(scene::polyline(&std::mem::take(&mut run), false));
            } else {
                run.clear();
            }
        }
        if run.len() > 1 {
            subpaths.push(scene::polyline(&run, false));
        }
        let mut mob = Mob::path(subpaths, false, style(args, Style::outline(default))?);
        mob.meta = Meta::Graph {
            map,
            func: Rc::clone(func),
            start: first.unwrap_or([0.0, 0.0]),
            end: last.unwrap_or([0.0, 0.0]),
        };
        self.add(mob)
    }

    fn axes_method(&mut self, id: usize, name: &str, args: &Args) -> Result<Value, Error> {
        let Meta::Axes {
            x,
            map,
            x_axis,
            y_axis,
            ..
        } = self.arena.get(id).meta.clone()
        else {
            return Err(Error::unsupported(format!("`.{name}()`")));
        };
        match name {
            "plot" | "get_graph" => {
                args.only(&["function", "x_range", "use_smoothing", "discontinuities"])?;
                let Some(Value::Lambda(f)) = args.get(0, "function") else {
                    return Err(Error::unsupported("plotting something other than a lambda"));
                };
                let f = Rc::clone(f);
                let r = range(args.get(1, "x_range"), x)?;
                self.graph(&f, map, r, args, WHITE)
            }
            "c2p" | "coords_to_point" | "n2p" | "number_to_point" => {
                let coords: Vec<f64> = match args.pos.as_slice() {
                    [Value::List(items)] => items.iter().map(num).collect::<Result<_, _>>()?,
                    [Value::Point(p)] => p.to_vec(),
                    items => items.iter().map(num).collect::<Result<_, _>>()?,
                };
                let (cx, cy) = (
                    coords.first().copied().unwrap_or(0.0),
                    coords.get(1).copied().unwrap_or(0.0),
                );
                Ok(Value::Point(map.apply([cx, cy])))
            }
            "get_origin" => Ok(Value::Point(map.apply([0.0, 0.0]))),
            "get_x_axis" => Ok(Value::Mob(x_axis)),
            "get_y_axis" => Ok(Value::Mob(y_axis)),
            "i2gp" | "input_to_graph_point" => {
                let at = args.num(0, "x", 0.0)?;
                let Some(Value::Mob(g)) = args.get(1, "graph") else {
                    return Err(Error::unsupported("input_to_graph_point without a graph"));
                };
                self.graph_point(*g, at).map(Value::Point)
            }
            "get_graph_label" => self.graph_label(args),
            "get_axis_labels" => {
                let xl = self.label_arg(args.get(0, "x_label"), "x")?;
                let yl = self.label_arg(args.get(1, "y_label"), "y")?;
                for (label, axis) in [(xl, x_axis), (yl, y_axis)] {
                    let corner = self.arena.critical(axis, [1.0, 1.0]);
                    let next = positional(vec![
                        Value::Point(corner),
                        Value::Point([1.0, 1.0]),
                        Value::Num(0.1),
                    ]);
                    self.mob_method(label, "next_to", &next)?;
                }
                self.add(Mob::group(vec![xl, yl]))
            }
            "add_coordinates" => self.add_coordinates(id),
            _ => Err(Error::unsupported(format!("`.{name}()`"))),
        }
    }

    fn graph_label(&mut self, args: &Args) -> Result<Value, Error> {
        args.only(&["graph", "label", "x_val", "direction", "buff", "dot"])?;
        let Some(Value::Mob(g)) = args.get(0, "graph") else {
            return Err(Error::unsupported("get_graph_label without a graph"));
        };
        let g = *g;
        let label = match args.get(1, "label") {
            Some(Value::Mob(m)) => *m,
            Some(Value::Str(s)) => self.math_label(s)?,
            _ => self.math_label("f(x)")?,
        };
        if args.key("color").is_none() {
            if let Some(Item::Path { style, .. }) = self.arena.get(g).items.first() {
                let c = style.stroke;
                self.arena.each_item(label, |i| set_color(i, c));
            }
        } else if let Some(c) = args.key("color") {
            let c = color(c)?;
            self.arena.each_item(label, |i| set_color(i, c));
        }
        let at = match args.get(2, "x_val") {
            Some(v) if !matches!(v, Value::None) => self.graph_point(g, num(v)?)?,
            _ => match self.arena.get(g).meta {
                Meta::Graph { end, .. } => end,
                _ => [0.0, 0.0],
            },
        };
        let next = positional(vec![
            Value::Point(at),
            Value::Point(args.point(3, "direction", [1.0, 0.0])?),
            Value::Num(args.num(4, "buff", NEXT_TO_BUFF)?),
        ]);
        self.mob_method(label, "next_to", &next)?;
        Ok(Value::Mob(label))
    }

    fn add_coordinates(&mut self, id: usize) -> Result<Value, Error> {
        let Meta::Axes {
            x,
            y,
            map,
            x_axis,
            y_axis,
        } = self.arena.get(id).meta.clone()
        else {
            return Err(Error::unsupported(
                "add_coordinates on something without axes",
            ));
        };
        let cross = [x[0].max(0.0).min(x[1]), y[0].max(0.0).min(y[1])];
        for (axis, r, horizontal) in [(x_axis, x, true), (y_axis, y, false)] {
            let tip = self.arena.get(axis).items.len() > 1;
            let from = if horizontal { cross[0] } else { cross[1] };
            let decimals = decimals(r[2]);
            for v in steps(r, from) {
                if (v - from).abs() < 1e-9 || (tip && (v - r[1]).abs() < 1e-9) {
                    continue;
                }
                let font = Args {
                    pos: Vec::new(),
                    key: vec![("font_size".into(), Value::Num(36.0))],
                };
                let label = self.arena.add(tex_mob(
                    &format!("{v:.decimals$}"),
                    true,
                    &font,
                    "axis numbers",
                )?)?;
                let (p, side) = if horizontal {
                    (map.apply([v, cross[1]]), [0.0, -1.0])
                } else {
                    (map.apply([cross[0], v]), [-1.0, 0.0])
                };
                let next = positional(vec![
                    Value::Point(p),
                    Value::Point(side),
                    Value::Num(NEXT_TO_BUFF),
                ]);
                self.mob_method(label, "next_to", &next)?;
                self.arena.get_mut(axis).children.push(label);
            }
        }
        Ok(Value::Mob(id))
    }

    fn graph_point(&mut self, g: usize, x: f64) -> Result<P, Error> {
        let Meta::Graph { map, func, .. } = self.arena.get(g).meta.clone() else {
            return Err(Error::unsupported(
                "a point on something that is not a graph",
            ));
        };
        let y = num(&self.apply(&func, &[Value::Num(x)])?)?;
        Ok(map.apply([x, y]))
    }

    fn math_label(&mut self, tex: &str) -> Result<usize, Error> {
        let m = tex_mob(tex, true, &Args::default(), "this label")?;
        self.arena.add(m)
    }

    fn label_arg(&mut self, v: Option<&Value>, default: &str) -> Result<usize, Error> {
        match v {
            Some(Value::Mob(m)) => Ok(*m),
            Some(Value::Str(s)) => self.math_label(s),
            _ => self.math_label(default),
        }
    }
}

/// The plain shapes: those made from numbers alone. `None` for any other
/// name.
fn shape(name: &str, args: &Args) -> Result<Option<Mob>, Error> {
    let path = |subpaths: Vec<Vec<Cubic>>, closed, color, extra: &[&str]| {
        args.only(extra)?;
        Ok::<Mob, Error>(Mob::path(
            subpaths,
            closed,
            style(args, Style::outline(color))?,
        ))
    };
    Ok(Some(match name {
        "Circle" => {
            let r = args.num(0, "radius", 1.0)?;
            path(vec![scene::circle(r)], true, RED, &["radius"])?
        }
        "Square" => {
            let s = args.num(0, "side_length", 2.0)?;
            path(vec![scene::rect(s, s)], true, WHITE, &["side_length"])?
        }
        "Rectangle" => {
            let (w, h) = (
                args.num(usize::MAX, "width", 4.0)?,
                args.num(usize::MAX, "height", 2.0)?,
            );
            let mut m = path(vec![scene::rect(w, h)], true, WHITE, &["width", "height"])?;
            if let Some(c) = args.pos.first() {
                recolor(&mut m, color(c)?);
            }
            m
        }
        "RoundedRectangle" => {
            let r = args.num(0, "corner_radius", 0.5)?;
            let (w, h) = (args.num(1, "width", 4.0)?, args.num(2, "height", 2.0)?);
            path(
                vec![scene::rounded_rect(w, h, r)],
                true,
                WHITE,
                &["corner_radius", "width", "height"],
            )?
        }
        "Ellipse" => {
            let (w, h) = (args.num(0, "width", 2.0)?, args.num(1, "height", 1.0)?);
            let mut m = path(vec![scene::circle(1.0)], true, RED, &["width", "height"])?;
            scale_items(&mut m, w / 2.0, h / 2.0);
            m
        }
        "Dot" => {
            args.only(&["point", "radius"])?;
            let at = args.point(0, "point", [0.0, 0.0])?;
            let r = args.num(1, "radius", 0.08)?;
            let mut circle = scene::circle(r);
            for p in circle.iter_mut().flatten() {
                *p = [p[0] + at[0], p[1] + at[1]];
            }
            Mob::path(vec![circle], true, style(args, Style::solid(WHITE))?)
        }
        "Triangle" => path(
            vec![polygon(&scene::regular(3, 1.0, PI / 2.0))],
            true,
            BLUE,
            &[],
        )?,
        "RegularPolygon" => {
            let n = count(args.get(0, "n"), 6)?;
            let start = if n % 2 == 1 { PI / 2.0 } else { 0.0 };
            path(
                vec![polygon(&scene::regular(n, 1.0, start))],
                true,
                BLUE,
                &["n"],
            )?
        }
        "Polygon" => {
            let points = args.pos.iter().map(point).collect::<Result<Vec<_>, _>>()?;
            path(vec![polygon(&points)], true, BLUE, &[])?
        }
        "Star" => star(args)?,
        "Arc" => {
            let r = args.num(0, "radius", 1.0)?;
            let start = args.num(1, "start_angle", 0.0)?;
            let sweep = args.num(2, "angle", TAU / 4.0)?;
            let c = args.point(usize::MAX, "arc_center", [0.0, 0.0])?;
            path(
                vec![scene::arc(c, r, start, sweep)],
                false,
                WHITE,
                &["radius", "start_angle", "angle", "arc_center"],
            )?
        }
        "Annulus" | "AnnularSector" | "Sector" => annulus(name, args)?,
        "Line" | "DashedLine" => line(name, args)?,
        "Arrow" | "Vector" | "DoubleArrow" => arrow(name, args)?,
        _ => return Ok(None),
    }))
}

fn annulus(name: &str, args: &Args) -> Result<Mob, Error> {
    args.only(&[
        "inner_radius",
        "outer_radius",
        "radius",
        "angle",
        "start_angle",
    ])?;
    let (inner, outer) = if name == "Sector" {
        (0.0, args.num(0, "radius", 1.0)?)
    } else {
        (
            args.num(0, "inner_radius", 1.0)?,
            args.num(1, "outer_radius", 2.0)?,
        )
    };
    let subpaths = if name == "Annulus" {
        vec![scene::circle(outer), scene::circle(inner)]
    } else {
        let sweep = args.num(usize::MAX, "angle", TAU / 4.0)?;
        let start = args.num(usize::MAX, "start_angle", 0.0)?;
        let mut out = scene::arc([0.0, 0.0], outer, start, sweep);
        let end = out.last().map_or([outer, 0.0], |c| c[3]);
        let inner_arc = scene::arc([0.0, 0.0], inner, start + sweep, -sweep);
        let inner_start = inner_arc.first().map_or([0.0, 0.0], |c| c[0]);
        out.push(scene::line(end, inner_start));
        out.extend(inner_arc);
        vec![out]
    };
    Ok(Mob::path(subpaths, true, style(args, Style::solid(WHITE))?))
}

/// Words, formulas and numbers. `None` for any other name.
fn text(name: &str, args: &Args) -> Result<Option<Mob>, Error> {
    Ok(Some(match name {
        "Text" | "MarkupText" => {
            args.only(&[
                "text",
                "font_size",
                "font",
                "weight",
                "slant",
                "line_spacing",
            ])?;
            let text = match args.get(0, "text") {
                Some(Value::Str(s)) => s.clone(),
                _ => return Err(Error::unsupported("Text without a string")),
            };
            if name == "MarkupText" {
                return Err(Error::unsupported("MarkupText"));
            }
            tex_mob(&text_tex(&text), false, args, "this text")?
        }
        "MathTex" | "Tex" => {
            args.only(&["font_size", "tex_environment", "arg_separator"])?;
            let parts = args
                .pos
                .iter()
                .map(|v| match v {
                    Value::Str(s) => Ok(s.clone()),
                    _ => Err(Error::unsupported("TeX that is not a string")),
                })
                .collect::<Result<Vec<_>, _>>()?;
            if name == "Tex" {
                let joined = parts.concat();
                tex_mob(&format!("\\text{{{joined}}}"), false, args, "this TeX")?
            } else {
                let joined = parts.join(" ");
                let tex = if joined.contains("\\\\") || joined.contains('&') {
                    format!("\\begin{{aligned}}{joined}\\end{{aligned}}")
                } else {
                    joined
                };
                tex_mob(&tex, true, args, "this formula")?
            }
        }
        "Integer" | "DecimalNumber" => {
            args.only(&["number", "num_decimal_places", "font_size"])?;
            let n = args.num(0, "number", 0.0)?;
            let places = if name == "Integer" {
                0
            } else {
                count(args.get(1, "num_decimal_places"), 2)?
            };
            tex_mob(&format!("{n:.places$}"), true, args, "this number")?
        }
        _ => return Ok(None),
    }))
}

fn star(args: &Args) -> Result<Mob, Error> {
    let n = count(args.get(0, "n"), 5)?;
    let outer = args.num(1, "outer_radius", 1.0)?;
    let inner = match args.get(2, "inner_radius") {
        Some(v) if !matches!(v, Value::None) => num(v)?,
        _ => outer * (TAU / n as f64).cos() / (PI / n as f64).cos(),
    };
    let points: Vec<P> = (0..2 * n)
        .map(|i| {
            let r = if i % 2 == 0 { outer } else { inner };
            let ang = PI / 2.0 + PI * i as f64 / n as f64;
            [r * ang.cos(), r * ang.sin()]
        })
        .collect();
    args.only(&["n", "outer_radius", "inner_radius"])?;
    Ok(Mob::path(
        vec![polygon(&points)],
        true,
        style(args, Style::outline(BLUE))?,
    ))
}

/// The length of an arrow's tip, and of an axis's.
const TIP: f64 = 0.35;

#[derive(Debug, Clone)]
struct AxisStyle {
    tip: bool,
    ticks: bool,
    numbers: bool,
    color: Rgb,
    stroke_width: f64,
    font_size: f64,
}

impl AxisStyle {
    fn new(tip: bool, ticks: bool) -> Self {
        Self {
            tip,
            ticks,
            numbers: false,
            color: WHITE,
            stroke_width: 2.0,
            font_size: 36.0,
        }
    }
}

fn axis_config(v: Option<&Value>, mut s: AxisStyle) -> Result<AxisStyle, Error> {
    let Some(v) = v else { return Ok(s) };
    let Value::Dict(items) = v else {
        return Err(Error::unsupported("axis_config that is not a dict"));
    };
    for (k, v) in items {
        match k.as_str() {
            "include_numbers" => s.numbers = truthy(Some(v)),
            "include_tip" => s.tip = truthy(Some(v)),
            "include_ticks" => s.ticks = truthy(Some(v)),
            "color" | "stroke_color" => s.color = color(v)?,
            "stroke_width" => s.stroke_width = num(v)?,
            "font_size" => s.font_size = num(v)?,
            _ => return Err(Error::unsupported(format!("`{k}` in axis_config"))),
        }
    }
    Ok(s)
}

/// The values from `r`'s start to its end in steps of its step, counted
/// out from `from` so that `from` itself is one of them.
fn steps(r: [f64; 3], from: f64) -> Vec<f64> {
    let step = r[2].abs();
    if step <= 0.0 || ((r[1] - r[0]) / step) > 1000.0 {
        return Vec::new();
    }
    let k0 = ((r[0] - from) / step - 1e-9).ceil() as i64;
    let k1 = ((r[1] - from) / step + 1e-9).floor() as i64;
    (k0..=k1).map(|k| from + k as f64 * step).collect()
}

fn decimals(step: f64) -> usize {
    (0..4)
        .find(|d| {
            let k = 10f64.powi(*d);
            ((step * k).round() - step * k).abs() < 1e-9
        })
        .map_or(2, |d| d as usize)
}

fn range(value: Option<&Value>, default: [f64; 3]) -> Result<[f64; 3], Error> {
    match value {
        None | Some(Value::None) => Ok(default),
        Some(Value::List(items)) => {
            let ends: Vec<f64> = items.iter().map(num).collect::<Result<_, _>>()?;
            match ends.as_slice() {
                [lo, hi] if hi > lo => Ok([*lo, *hi, default[2]]),
                [lo, hi, step] if hi > lo && *step > 0.0 => Ok([*lo, *hi, *step]),
                _ => Err(Error::unsupported("that range")),
            }
        }
        Some(_) => Err(Error::unsupported("a range that is not a list")),
    }
}

fn line(name: &str, args: &Args) -> Result<Mob, Error> {
    args.only(&["start", "end", "buff", "dash_length"])?;
    let (start, end) = (
        args.point(0, "start", [-1.0, 0.0])?,
        args.point(1, "end", [1.0, 0.0])?,
    );
    let buff = args.num(usize::MAX, "buff", 0.0)?;
    let dir = unit([end[0] - start[0], end[1] - start[1]]);
    let (s, e) = (
        [start[0] + dir[0] * buff, start[1] + dir[1] * buff],
        [end[0] - dir[0] * buff, end[1] - dir[1] * buff],
    );
    let mut mob = Mob::new(vec![Item::Path {
        subpaths: vec![vec![scene::line(s, e)]],
        closed: false,
        style: style(args, Style::outline(WHITE))?,
        dash: (name == "DashedLine")
            .then(|| args.num(usize::MAX, "dash_length", 0.05))
            .transpose()?,
    }]);
    mob.meta = Meta::Line { start: s, end: e };
    Ok(mob)
}

fn arrow(name: &str, args: &Args) -> Result<Mob, Error> {
    args.only(&[
        "start",
        "end",
        "direction",
        "buff",
        "tip_length",
        "max_tip_length_to_length_ratio",
        "max_stroke_width_to_length_ratio",
    ])?;
    let (start, end, buff) = if name == "Vector" {
        (
            [0.0, 0.0],
            args.point(0, "direction", [1.0, 0.0])?,
            args.num(usize::MAX, "buff", 0.0)?,
        )
    } else {
        (
            args.point(0, "start", [-1.0, 0.0])?,
            args.point(1, "end", [1.0, 0.0])?,
            args.num(usize::MAX, "buff", 0.25)?,
        )
    };
    let dir = unit([end[0] - start[0], end[1] - start[1]]);
    let s = [start[0] + dir[0] * buff, start[1] + dir[1] * buff];
    let e = [end[0] - dir[0] * buff, end[1] - dir[1] * buff];
    let length = (e[0] - s[0]).hypot(e[1] - s[1]);
    let tip_ratio = args.num(usize::MAX, "max_tip_length_to_length_ratio", 0.25)?;
    let tip_len = args
        .num(usize::MAX, "tip_length", TIP)?
        .min(tip_ratio * length);
    let double = name == "DoubleArrow";
    let shaft = if double {
        length - 2.0 * tip_len
    } else {
        length
    };
    let stroke_ratio = args.num(usize::MAX, "max_stroke_width_to_length_ratio", 5.0)?;
    let mut base = Style::outline(WHITE);
    base.stroke_width = 6.0_f64.min(stroke_ratio * shaft.max(0.0));
    let st = style(args, base)?;
    let line_end = [e[0] - dir[0] * tip_len, e[1] - dir[1] * tip_len];
    let line_start = if double {
        [s[0] + dir[0] * tip_len, s[1] + dir[1] * tip_len]
    } else {
        s
    };
    let mut items = vec![Item::Path {
        subpaths: vec![vec![scene::line(line_start, line_end)]],
        closed: false,
        style: st,
        dash: None,
    }];
    items.push(tip(e, dir, tip_len, st.stroke));
    if double {
        items.push(tip(s, [-dir[0], -dir[1]], tip_len, st.stroke));
    }
    let mut mob = Mob::new(items);
    mob.meta = Meta::Line { start: s, end: e };
    Ok(mob)
}

/// A filled triangle with its point at `at`, facing `dir`.
fn tip(at: P, dir: P, length: f64, c: Rgb) -> Item {
    let base = [at[0] - dir[0] * length, at[1] - dir[1] * length];
    let half = [-dir[1] * length / 2.0, dir[0] * length / 2.0];
    Item::Path {
        subpaths: vec![polygon(&[
            at,
            [base[0] + half[0], base[1] + half[1]],
            [base[0] - half[0], base[1] - half[1]],
        ])],
        closed: true,
        style: Style::solid(c),
        dash: None,
    }
}

fn polygon(points: &[P]) -> Vec<Cubic> {
    scene::polyline(points, true)
}

fn unit(v: P) -> P {
    let n = v[0].hypot(v[1]);
    if n > 0.0 {
        [v[0] / n, v[1] / n]
    } else {
        [1.0, 0.0]
    }
}

/// Text as TeX: each line in `\text`, special characters escaped.
fn text_tex(text: &str) -> String {
    let escape = |line: &str| {
        let mut out = String::new();
        for c in line.chars() {
            match c {
                '#' | '$' | '%' | '&' | '_' | '{' | '}' => {
                    out.push('\\');
                    out.push(c);
                }
                '\\' => out.push_str("\\textbackslash{}"),
                '^' => out.push_str("\\^{}"),
                '~' => out.push_str("\\~{}"),
                c => out.push(c),
            }
        }
        format!("\\text{{{out}}}")
    };
    let lines: Vec<String> = text.lines().map(escape).collect();
    if lines.len() > 1 {
        format!("\\begin{{array}}{{l}}{}\\end{{array}}", lines.join("\\\\"))
    } else {
        lines.into_iter().next().unwrap_or_default()
    }
}

fn tex_mob(tex: &str, display: bool, args: &Args, what: &str) -> Result<Mob, Error> {
    let font_size = args.num(usize::MAX, "font_size", 48.0)?;
    let em = font_size * EM_PER_POINT;
    let Some((w, h)) = math::measure(tex, display) else {
        return Err(Error::unsupported(what));
    };
    let color = match args.key("color") {
        Some(c) => color(c)?,
        None => WHITE,
    };
    let opacity = match args.key("fill_opacity") {
        Some(o) => num(o)?,
        None => 1.0,
    };
    Ok(Mob::new(vec![Item::Tex {
        tex: tex.to_owned(),
        display,
        center: [0.0, 0.0],
        em,
        angle: 0.0,
        size: [f64::from(w) * em, f64::from(h) * em],
        color,
        opacity,
    }]))
}

fn style(args: &Args, mut s: Style) -> Result<Style, Error> {
    if let Some(c) = args.key("color") {
        let c = color(c)?;
        s.stroke = c;
        s.fill = c;
    }
    if let Some(c) = args.key("fill_color") {
        s.fill = color(c)?;
    }
    if let Some(o) = args.key("fill_opacity") {
        s.fill_opacity = num(o)?;
    }
    if let Some(c) = args.key("stroke_color") {
        s.stroke = color(c)?;
    }
    if let Some(w) = args.key("stroke_width") {
        s.stroke_width = num(w)?;
    }
    if let Some(o) = args.key("stroke_opacity") {
        s.stroke_opacity = num(o)?;
    }
    Ok(s)
}

fn set_color(item: &mut Item, c: Rgb) {
    match item {
        Item::Path { style, .. } => {
            style.stroke = c;
            style.fill = c;
        }
        Item::Tex { color, .. } => *color = c,
    }
}

fn recolor(mob: &mut Mob, c: Rgb) {
    mob.items.iter_mut().for_each(|i| set_color(i, c));
}

fn scale_items(mob: &mut Mob, sx: f64, sy: f64) {
    for item in &mut mob.items {
        if let Item::Path { subpaths, .. } = item {
            for p in subpaths.iter_mut().flatten().flatten() {
                *p = [p[0] * sx, p[1] * sy];
            }
        }
    }
}

fn shift_items(mob: &mut Mob, by: P) {
    for item in &mut mob.items {
        if let Item::Path { subpaths, .. } = item {
            for p in subpaths.iter_mut().flatten().flatten() {
                *p = [p[0] + by[0], p[1] + by[1]];
            }
        }
    }
}

/// Arguments given by position only.
fn positional(pos: Vec<Value>) -> Args {
    Args {
        pos,
        key: Vec::new(),
    }
}

// ----- values -----

fn num(v: &Value) -> Result<f64, Error> {
    match v {
        Value::Num(n) => Ok(*n),
        Value::Bool(b) => Ok(f64::from(u8::from(*b))),
        _ => Err(Error::unsupported("a number that is not one")),
    }
}

fn count(v: Option<&Value>, default: usize) -> Result<usize, Error> {
    match v {
        None => Ok(default),
        Some(v) => {
            let n = num(v)?;
            if n >= 0.0 && n.fract() == 0.0 && n <= 1000.0 {
                Ok(n as usize)
            } else {
                Err(Error::unsupported("that count"))
            }
        }
    }
}

fn truthy(v: Option<&Value>) -> bool {
    match v {
        Some(Value::Bool(b)) => *b,
        Some(Value::Num(n)) => *n != 0.0,
        _ => false,
    }
}

fn point(v: &Value) -> Result<P, Error> {
    match v {
        Value::Point(p) => Ok(*p),
        Value::List(items) if (2..=3).contains(&items.len()) => {
            Ok([num(&items[0])?, num(&items[1])?])
        }
        Value::Mob(_) => Err(Error::unsupported("a mobject where a point goes")),
        _ => Err(Error::unsupported("a point that is not one")),
    }
}

fn color(v: &Value) -> Result<Rgb, Error> {
    match v {
        Value::Color(c) => Ok(*c),
        Value::Str(s) => {
            colors::parse(s).ok_or_else(|| Error::unsupported(format!("the colour {s:?}")))
        }
        Value::List(items) if !items.is_empty() => color(&items[0]),
        _ => Err(Error::unsupported("a colour that is not one")),
    }
}

fn mobs(values: &[Value]) -> Result<Vec<usize>, Error> {
    let mut out = Vec::new();
    for v in values {
        match v {
            Value::Mob(id) => out.push(*id),
            Value::List(items) => out.extend(mobs(items)?),
            _ => return Err(Error::unsupported("something that is not a mobject")),
        }
    }
    Ok(out)
}

fn index(arena: &Arena, obj: &Value, i: &Value) -> Result<Value, Error> {
    let i = num(i)?;
    let pick = |len: usize| {
        let k = if i < 0.0 { len as f64 + i } else { i };
        (k >= 0.0 && k.fract() == 0.0 && (k as usize) < len).then_some(k as usize)
    };
    match obj {
        Value::List(items) => pick(items.len())
            .map(|k| items[k].clone())
            .ok_or_else(|| Error::unsupported("that index")),
        Value::Mob(id) => {
            let children = &arena.get(*id).children;
            pick(children.len())
                .map(|k| Value::Mob(children[k]))
                .ok_or_else(|| Error::unsupported("that index"))
        }
        Value::Point(p) => pick(3)
            .map(|k| Value::Num(p.get(k).copied().unwrap_or(0.0)))
            .ok_or_else(|| Error::unsupported("that index")),
        _ => Err(Error::unsupported("indexing that")),
    }
}

fn arith(op: char, left: &Value, right: &Value) -> Result<Value, Error> {
    let apply = |x: f64, y: f64| -> Result<f64, Error> {
        Ok(match op {
            '+' => x + y,
            '-' => x - y,
            '*' => x * y,
            '/' => x / y,
            'f' => (x / y).floor(),
            '%' => x.rem_euclid(y),
            '^' => x.powf(y),
            _ => return Err(Error::unsupported(format!("`{op}`"))),
        })
    };
    match (left, right) {
        (Value::Num(x), Value::Num(y)) => apply(*x, *y).map(Value::Num),
        (Value::Point(p), Value::Point(q)) if matches!(op, '+' | '-' | '*' | '/') => {
            Ok(Value::Point([apply(p[0], q[0])?, apply(p[1], q[1])?]))
        }
        (Value::Point(p), Value::Num(k)) if matches!(op, '*' | '/') => {
            Ok(Value::Point([apply(p[0], *k)?, apply(p[1], *k)?]))
        }
        (Value::Num(k), Value::Point(p)) if op == '*' => Ok(Value::Point([k * p[0], k * p[1]])),
        (Value::Str(x), Value::Str(y)) if op == '+' => Ok(Value::Str(format!("{x}{y}"))),
        (Value::List(x), Value::List(y)) if op == '+' => {
            Ok(Value::List(x.iter().chain(y).cloned().collect()))
        }
        _ => Err(Error::unsupported(format!("`{op}` between those"))),
    }
}

fn builtin(name: &str, args: &Args) -> Result<Value, Error> {
    let nums = || args.pos.iter().map(num).collect::<Result<Vec<_>, _>>();
    Ok(match name {
        "abs" => Value::Num(num(args.get(0, "x").unwrap_or(&Value::None))?.abs()),
        "min" => Value::Num(nums()?.into_iter().fold(f64::INFINITY, f64::min)),
        "max" => Value::Num(nums()?.into_iter().fold(f64::NEG_INFINITY, f64::max)),
        "round" => {
            let x = num(args.get(0, "number").unwrap_or(&Value::None))?;
            let d = args.num(1, "ndigits", 0.0)?;
            let k = 10f64.powf(d);
            Value::Num((x * k).round() / k)
        }
        "float" => Value::Num(num(args.get(0, "x").unwrap_or(&Value::None))?),
        "int" => Value::Num(num(args.get(0, "x").unwrap_or(&Value::None))?.trunc()),
        "len" => match args.pos.first() {
            Some(Value::List(items)) => Value::Num(items.len() as f64),
            Some(Value::Str(s)) => Value::Num(s.chars().count() as f64),
            _ => return Err(Error::unsupported("len() of that")),
        },
        "str" => match args.pos.first() {
            Some(Value::Num(n)) if n.fract() == 0.0 => Value::Str(format!("{n:.0}")),
            Some(Value::Num(n)) => Value::Str(n.to_string()),
            Some(Value::Str(s)) => Value::Str(s.clone()),
            _ => return Err(Error::unsupported("str() of that")),
        },
        _ => return Err(Error::unsupported(format!("`{name}`"))),
    })
}

const MATH_FUNCS: [&str; 18] = [
    "sin", "cos", "tan", "arcsin", "arccos", "arctan", "asin", "acos", "atan", "sinh", "cosh",
    "tanh", "exp", "log", "sqrt", "abs", "floor", "ceil",
];

fn numpy(f: &str, args: &Args) -> Result<Value, Error> {
    if f == "array" {
        return match args.pos.first() {
            Some(v @ Value::List(_)) => point(v).map(Value::Point),
            _ => Err(Error::unsupported("np.array of that")),
        };
    }
    if !MATH_FUNCS.contains(&f) {
        return Err(Error::unsupported(format!("`np.{f}`")));
    }
    let x = num(args.pos.first().unwrap_or(&Value::None))?;
    Ok(Value::Num(match f {
        "sin" => x.sin(),
        "cos" => x.cos(),
        "tan" => x.tan(),
        "arcsin" | "asin" => x.asin(),
        "arccos" | "acos" => x.acos(),
        "arctan" | "atan" => x.atan(),
        "sinh" => x.sinh(),
        "cosh" => x.cosh(),
        "tanh" => x.tanh(),
        "exp" => x.exp(),
        "log" => x.ln(),
        "sqrt" => x.sqrt(),
        "abs" => x.abs(),
        "floor" => x.floor(),
        _ => x.ceil(),
    }))
}

fn animation(name: &str, args: &Args) -> Result<Anim, Error> {
    let first_mobs = || mobs(&args.pos[..args.pos.len().min(1)]);
    Ok(match name {
        "Create"
        | "Write"
        | "FadeIn"
        | "DrawBorderThenFill"
        | "GrowFromCenter"
        | "GrowFromEdge"
        | "GrowFromPoint"
        | "GrowArrow"
        | "SpinInFromNothing"
        | "AddTextLetterByLetter"
        | "AddTextWordByWord"
        | "Indicate"
        | "Circumscribe"
        | "Wiggle"
        | "ApplyWave"
        | "ShowIncreasingSubsets"
        | "ShowSubmobjectsOneByOne"
        | "Blink" => {
            if args.pos.first().is_some_and(|v| matches!(v, Value::Mob(_))) {
                Anim::Show(first_mobs()?)
            } else {
                return Err(Error::unsupported(format!("{name} of that")));
            }
        }
        "Flash" | "FocusOn" | "ShowPassingFlash" | "Wait" => Anim::Nothing,
        "FadeOut" | "Uncreate" | "Unwrite" | "ShrinkToCenter" => Anim::Hide(mobs(&args.pos)?),
        "Transform" | "ClockwiseTransform" | "CounterclockwiseTransform" => {
            match args.pos.as_slice() {
                [Value::Mob(from), Value::Mob(to), ..] => Anim::Transform {
                    from: *from,
                    to: *to,
                },
                _ => return Err(Error::unsupported(format!("{name} of those"))),
            }
        }
        "ReplacementTransform"
        | "FadeTransform"
        | "TransformMatchingShapes"
        | "TransformMatchingTex" => match args.pos.as_slice() {
            [Value::Mob(from), Value::Mob(to), ..] => Anim::Replace {
                from: *from,
                to: *to,
            },
            _ => return Err(Error::unsupported(format!("{name} of those"))),
        },
        "TransformFromCopy" => match args.pos.as_slice() {
            [Value::Mob(_), Value::Mob(to), ..] => Anim::Show(vec![*to]),
            _ => return Err(Error::unsupported("TransformFromCopy of those")),
        },
        "MoveToTarget" => match args.pos.first() {
            Some(Value::Mob(id)) => Anim::Target(*id),
            _ => return Err(Error::unsupported("MoveToTarget of that")),
        },
        "Rotate" => match args.pos.first() {
            Some(Value::Mob(id)) => Anim::Rotate {
                mob: *id,
                angle: args.num(1, "angle", PI)?,
                about: args.key("about_point").map(point).transpose()?,
            },
            _ => return Err(Error::unsupported("Rotate of that")),
        },
        "AnimationGroup" | "LaggedStart" | "Succession" => Anim::Group(
            args.pos
                .iter()
                .map(|v| match v {
                    Value::Anim(anim) => Ok(anim.clone()),
                    Value::Animate(id, calls) => Ok(Anim::Methods(*id, calls.clone())),
                    _ => Err(Error::unsupported(format!("{name} of that"))),
                })
                .collect::<Result<_, _>>()?,
        ),
        _ => return Err(Error::unsupported(format!("`{name}`"))),
    })
}

const CALLABLE: &[&str] = &[
    "Circle",
    "Square",
    "Rectangle",
    "RoundedRectangle",
    "Ellipse",
    "Dot",
    "Triangle",
    "RegularPolygon",
    "Polygon",
    "Star",
    "Arc",
    "Annulus",
    "AnnularSector",
    "Sector",
    "Line",
    "DashedLine",
    "Arrow",
    "Vector",
    "DoubleArrow",
    "Cross",
    "SurroundingRectangle",
    "Text",
    "MarkupText",
    "MathTex",
    "Tex",
    "Integer",
    "DecimalNumber",
    "VGroup",
    "Group",
    "Axes",
    "NumberPlane",
    "NumberLine",
    "FunctionGraph",
    "ManimColor",
    "interpolate_color",
    "abs",
    "min",
    "max",
    "round",
    "float",
    "int",
    "len",
    "str",
];

const ANIMS: &[&str] = &[
    "Create",
    "Write",
    "FadeIn",
    "DrawBorderThenFill",
    "GrowFromCenter",
    "GrowFromEdge",
    "GrowFromPoint",
    "GrowArrow",
    "SpinInFromNothing",
    "AddTextLetterByLetter",
    "AddTextWordByWord",
    "Indicate",
    "Circumscribe",
    "Wiggle",
    "ApplyWave",
    "ShowIncreasingSubsets",
    "ShowSubmobjectsOneByOne",
    "Blink",
    "Flash",
    "FocusOn",
    "ShowPassingFlash",
    "Wait",
    "FadeOut",
    "Uncreate",
    "Unwrite",
    "ShrinkToCenter",
    "Transform",
    "ClockwiseTransform",
    "CounterclockwiseTransform",
    "ReplacementTransform",
    "FadeTransform",
    "TransformMatchingShapes",
    "TransformMatchingTex",
    "TransformFromCopy",
    "MoveToTarget",
    "Rotate",
    "AnimationGroup",
    "LaggedStart",
    "Succession",
];

const RATE_FUNCS: &[&str] = &[
    "smooth",
    "linear",
    "rush_into",
    "rush_from",
    "slow_into",
    "double_smooth",
    "there_and_back",
    "there_and_back_with_pause",
    "running_start",
    "not_quite_there",
    "wiggle",
    "squish_rate_func",
    "lingering",
    "exponential_decay",
    "rate_functions",
];
