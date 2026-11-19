//! Drawing wood into the skeleton: limbs, trunks, deadwood, winter twigs.
//!
//! Every helper appends nodes (with depth) to a [`Skeleton`]; positions are
//! canvas units, y down, depth toward the viewer positive.

use crate::growth::life::Lang;
use crate::growth::{JIN, Node, P, Skeleton};
use crate::math;
use crate::rng::Rng;
use crate::style::{CENTER_X, SOIL_Y};

/// Length of one wood segment.
pub const STEP: f64 = 3.0;
/// Half a turn.
pub const PI: f64 = math::TAU / 2.0;
/// Limbs never dip below this line: they would sink into the pot.
pub const FLOOR: f64 = SOIL_Y - 20.0;

/// A skeleton holding only the root, at the soil in the middle.
pub fn root_only() -> Skeleton {
    let mut sk = Skeleton::default();
    sk.nodes.push(Node {
        p: (CENTER_X, SOIL_Y),
        parent: None,
        own: 0.0,
        language: None,
        bleach: 1.0,
        strand: false,
    });
    sk.node_z.push(0.0);
    sk
}

/// Appends a node at `p`, depth `z`, grown from `parent`; returns its index.
pub fn add(sk: &mut Skeleton, p: P, z: f64, parent: usize, language: &Lang) -> usize {
    sk.nodes.push(Node {
        p,
        parent: Some(parent),
        own: 0.0,
        language: language.clone(),
        bleach: 1.0,
        strand: false,
    });
    sk.node_z.push(z);
    sk.nodes.len() - 1
}

/// Sets how bleached the dead wood grown since node `from` is.
pub fn bleach_from(sk: &mut Skeleton, from: usize, bleach: f64) {
    for n in &mut sk.nodes[from..] {
        if n.language.as_deref() == Some(JIN) {
            n.bleach = bleach;
        }
    }
}

/// The depth of node `i`.
pub fn zof(sk: &Skeleton, i: usize) -> f64 {
    sk.node_z.get(i).copied().unwrap_or(0.0)
}

/// The signed shortest turn from angle `from` to angle `to`.
fn turn(from: f64, to: f64) -> f64 {
    let mut d = (to - from) % math::TAU;
    if d > PI {
        d -= math::TAU;
    }
    if d < -PI {
        d += math::TAU;
    }
    d
}

/// A sinuous limb from node `from` to `to` at depth `tz`: an arc of `sag`
/// plus `waves` half-waves of `amp`, which also twist it in depth. Returns
/// its nodes, the last at `to`.
#[allow(clippy::too_many_arguments, reason = "the shape of a limb")]
pub fn limb_to(
    sk: &mut Skeleton,
    from: usize,
    to: P,
    tz: f64,
    sag: f64,
    amp: f64,
    waves: f64,
    phase: f64,
    lang: &Lang,
) -> Vec<usize> {
    let s = sk.nodes[from].p;
    let sz = zof(sk, from);
    let (dx, dy) = (to.0 - s.0, to.1 - s.1);
    let len = (dx * dx + dy * dy).sqrt().max(1e-6);
    let (nx, ny) = (-dy / len, dx / len);
    let steps = steps_for(len + amp * waves * 1.5);
    let mut at = from;
    let mut out = Vec::with_capacity(steps);
    for i in 1..=steps {
        let t = i as f64 / steps as f64;
        let env = math::sin(PI * t);
        let off = sag * 4.0 * t * (1.0 - t) + amp * math::sin(PI * waves * t + phase) * env;
        let zoff = amp * 0.7 * math::cos(PI * waves * t + phase) * env;
        let y = s.1 + dy * t + ny * off;
        let p = (
            s.0 + dx * t + nx * off,
            if s.1 < FLOOR { y.min(FLOOR) } else { y },
        );
        at = add(sk, p, sz + (tz - sz) * t + zoff, at, lang);
        out.push(at);
    }
    out
}

/// Segments for a length: at least two.
fn steps_for(len: f64) -> usize {
    segments(len, 2.0)
}

/// Segments of about `STEP` for `len`, at least `min`.
fn segments(len: f64, min: f64) -> usize {
    // Wood is at most a few hundred units long.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "small positive"
    )]
    let n = (len / STEP).ceil().clamp(min, 10_000.0) as usize;
    n
}

/// The node of `chain` nearest height `y` above the soil.
pub fn at_height(sk: &Skeleton, chain: &[usize], y: f64) -> usize {
    *chain
        .iter()
        .min_by(|a, b| {
            let da = (SOIL_Y - sk.nodes[**a].p.1 - y).abs();
            let db = (SOIL_Y - sk.nodes[**b].p.1 - y).abs();
            da.total_cmp(&db)
        })
        .unwrap_or(&0)
}

/// The node `frac` of the way along `chain`.
pub fn at_frac(chain: &[usize], frac: f64) -> usize {
    let last = chain.len().saturating_sub(1);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "0..=last"
    )]
    let i = (last as f64 * frac.clamp(0.0, 1.0)).round() as usize;
    chain[i.min(last)]
}

/// Root flare: the foot of the main trunk carries extra girth.
pub fn flare(sk: &mut Skeleton, chain: &[usize]) {
    let total: f64 = sk.nodes.iter().map(|n| n.own).sum();
    for (k, f) in [(1usize, 0.12), (2, 0.06), (3, 0.03)] {
        if let Some(&i) = chain.get(k) {
            sk.nodes[i].own += total * f;
        }
    }
}

/// A limb grown in 3D: azimuth around the trunk (0 = right, +pi/2 = toward
/// the viewer) and elevation (positive = up), wandering as it goes.
#[derive(Clone)]
pub struct Limb {
    at: usize,
    az: f64,
    el: f64,
    head_el: f64,
    head_az: f64,
    wander: f64,
    pull: f64,
    curl: f64,
    freq: f64,
    phase: f64,
    run: f64,
}

impl Limb {
    /// A limb from node `at`, heading `az`, `el`.
    pub fn new(at: usize, az: f64, el: f64) -> Self {
        Self {
            at,
            az,
            el,
            head_az: az,
            head_el: el,
            wander: 0.25,
            pull: 0.4,
            curl: 0.0,
            freq: 0.0,
            phase: 0.0,
            run: 0.0,
        }
    }

    /// Bend character from irregularity `r` (0 to 1.2): steady growth is
    /// straight, irregular growth curls.
    #[must_use]
    pub fn character(mut self, r: f64, rng: &mut Rng) -> Self {
        let r = r.min(1.0);
        self.wander = 0.06 + 0.3 * r;
        self.pull = 0.55 - 0.2 * r;
        self.curl = 0.04 + 0.62 * r;
        self.freq = 0.018 + 0.05 * r;
        self.phase = rng.range(0.0, math::TAU);
        self
    }

    /// Grows `length` more units.
    pub fn grow(&mut self, sk: &mut Skeleton, rng: &mut Rng, length: f64, lang: &Lang) {
        let mut left = length;
        while left > 1.0 {
            let seg = left.min(8.0);
            let curl = self.curl * math::sin(self.phase + self.run * self.freq);
            let t_el = (self.el
                + rng.range(-self.wander, self.wander)
                + self.pull * (self.head_el + curl - self.el))
                .clamp(-0.85, 1.25);
            let t_az = self.az
                + rng.range(-self.wander, self.wander) * 0.7
                + self.pull * turn(self.az, self.head_az)
                + 0.6 * curl * self.curl;
            let steps = segments(seg, 1.0);
            for s in 1..=steps {
                let f = s as f64 / steps as f64;
                let az = self.az + turn(self.az, t_az) * f;
                let el = self.el + (t_el - self.el) * f;
                let p = sk.nodes[self.at].p;
                let z = zof(sk, self.at);
                let l = seg / steps as f64;
                let q = (
                    p.0 + math::cos(el) * math::cos(az) * l,
                    (p.1 - math::sin(el) * l).min(FLOOR),
                );
                self.at = add(sk, q, z + math::cos(el) * math::sin(az) * l, self.at, lang);
            }
            self.az = t_az;
            self.el = t_el;
            self.run += seg;
            left -= seg;
        }
    }
}

/// Deadwood on the trunk: a short, broken spur of jin carrying `amount`,
/// coming out from behind the trunk.
pub fn jin_limb(sk: &mut Skeleton, rng: &mut Rng, limb: Limb, len: f64, amount: f64) {
    dead_kinked(
        sk,
        rng,
        &limb,
        len.max(16.0) * 1.15,
        amount.clamp(60.0, 160.0),
        &[0.6, 0.4],
        false,
        BEHIND,
    );
}

/// A whole dead limb, the way jin looks on an old maple: an era in a
/// dropped language, stout, coming out from behind the trunk and reaching
/// up and out in weathered kinks to a broken tip, with a snapped-off fork.
pub fn dead_limb(sk: &mut Skeleton, rng: &mut Rng, limb: Limb, len: f64, amount: f64) {
    dead_kinked(
        sk,
        rng,
        &limb,
        len.max(28.0),
        amount.clamp(320.0, 900.0),
        &[0.42, 0.33, 0.25],
        true,
        BEHIND,
    );
}

/// How far behind the trunk dead wood starts, so it never meets the bark
/// in an abrupt joint.
const BEHIND: f64 = 16.0;

/// Dead wood in straight runs (`parts` of `len`) with sharp kinks, trending
/// upward, starting `behind` the trunk at `limb`'s node; tapers slowly
/// so it stays stout until it breaks off.
fn dead_kinked(
    sk: &mut Skeleton,
    rng: &mut Rng,
    limb: &Limb,
    len: f64,
    amount: f64,
    parts: &[f64],
    fork: bool,
    behind: f64,
) {
    let lang = Some(JIN.to_string());
    let mut theta = math::atan2(-math::sin(limb.el), math::cos(limb.el) * math::cos(limb.az));
    let side = if math::cos(theta) >= 0.0 { 1.0 } else { -1.0 };
    let start = sk.nodes[limb.at].p;
    let back = zof(sk, limb.at) - behind;
    // Out of sight first: from the trunk's heart straight back.
    let mut ids = limb_to(
        sk,
        limb.at,
        (start.0 + side * 3.0, start.1 - 2.0),
        back,
        0.0,
        0.0,
        1.0,
        0.0,
        &lang,
    );
    let mut at = *ids.last().unwrap_or(&limb.at);
    let mut fork_at = None;
    let mut turn_sign = rng.sign();
    for (k, part) in parts.iter().enumerate() {
        if k > 0 {
            theta += turn_sign * rng.range(0.3, 0.55) - 0.16 * side;
            turn_sign = -turn_sign;
        }
        let l = len * part;
        let p = sk.nodes[at].p;
        let to = (
            p.0 + math::cos(theta) * l,
            (p.1 + math::sin(theta) * l).min(FLOOR),
        );
        let tz = back + rng.range(-4.0, 4.0);
        let run = limb_to(
            sk,
            at,
            to,
            tz,
            0.0,
            0.6,
            1.0,
            rng.range(0.0, math::TAU),
            &lang,
        );
        at = *run.last().unwrap_or(&at);
        if k == 0 {
            fork_at = Some(at);
        }
        ids.extend(run);
    }
    taper(sk, &ids, amount, 1.1);
    // A fork that broke off: a short spike rising from the first kink.
    if fork && let Some(at) = fork_at {
        let up = theta - side * 0.9;
        let p = sk.nodes[at].p;
        let l = len * 0.24;
        let to = (p.0 + math::cos(up) * l, p.1 + math::sin(up) * l);
        let spike = limb_to(sk, at, to, zof(sk, at), 0.0, 0.3, 1.0, 0.0, &lang);
        taper(sk, &spike, amount * 0.12, 1.5);
    }
}

/// Spreads `amount` along `ids` so their girth tapers toward the last node;
/// a lower `power` keeps the wood stout for longer.
fn taper(sk: &mut Skeleton, ids: &[usize], amount: f64, power: f64) {
    let m = ids.len() as f64;
    let weight = |i: usize| math::pow(m - i as f64, power);
    let sum: f64 = (0..ids.len()).map(weight).sum::<f64>().max(1.0);
    for (i, &id) in ids.iter().enumerate() {
        sk.nodes[id].own += amount * weight(i) / sum;
    }
}

/// A smooth trunk in 3D from `from`: rises `h`, leaning `lean` and easing
/// back upright, bending with `amp`/`freq`, drifting `zd` in depth.
#[allow(clippy::too_many_arguments, reason = "the shape of a trunk")]
pub fn trunk3(
    sk: &mut Skeleton,
    from: usize,
    h: f64,
    lean: f64,
    amp: f64,
    freq: f64,
    phase: f64,
    zd: f64,
    lang: &Lang,
    early: f64,
) -> Vec<usize> {
    let (x0, y0) = sk.nodes[from].p;
    let z0 = zof(sk, from);
    let n = steps_for(h);
    let mut at = from;
    let mut out = vec![from];
    for i in 1..=n {
        let u = i as f64 / n as f64;
        // `early` moves the lean down toward the foot: a trunk bent over low
        // rather than near its top, reaching the same place.
        let shape =
            (1.0 - early) * u * (1.0 - 0.42 * u) + early * 0.58 * (1.0 - math::pow(1.0 - u, 3.0));
        let x = x0
            + h * math::sin(lean) * shape
            + amp
                * h
                * (math::sin(phase + freq * math::TAU * u) - math::sin(phase))
                * (1.0 - 0.35 * u);
        let y = y0 - h * u * math::cos(lean * 0.6);
        let z = z0
            + zd * u * (1.0 - 0.3 * u)
            + amp * h * 0.6 * (math::cos(phase + freq * math::TAU * u) - math::cos(phase));
        at = add(sk, (x, y), z, at, lang);
        out.push(at);
    }
    out
}

/// How a second trunk winds around the main one.
#[derive(Debug, Clone, Copy)]
pub struct Wind {
    /// Turns around the main trunk from the foot to the top.
    pub turns: f64,
    /// How far from the main trunk's line the strand runs, at the foot.
    pub radius: f64,
    /// Where the strand ends up parting from the trunk, 0 (foot) to 1 (top).
    pub part_at: f64,
    /// How far the free part leans out under its own weight, 0 to 1.
    pub sag: f64,
    /// Where around the trunk it runs, in turns.
    pub offset: f64,
}

/// A second trunk grown around the main trunk `around` from its node
/// `start`: wound up the trunk, then, as `peel` goes from 0 to 1, parting
/// from the top down to `wind.part_at`, sagging out to `side` under its own
/// weight before rising to `top`. Returns the strand's nodes from `start`
/// and the index in it where the free part begins.
#[allow(clippy::too_many_arguments, reason = "the shape of a trunk")]
pub fn strand(
    sk: &mut Skeleton,
    around: &[usize],
    start: usize,
    wind: Wind,
    peel: f64,
    side: f64,
    top: P,
    lang: &Lang,
) -> (Vec<usize>, usize) {
    let m = around.len() - 1;
    let span = (m - start).max(1) as f64;
    // It parts first just under the crown, where it shows, then lower.
    const SHOWS: f64 = 0.72;
    let part = if peel <= 0.0 {
        1.0
    } else {
        SHOWS - (SHOWS - wind.part_at.min(SHOWS)) * peel.clamp(0.0, 1.0)
    };
    // The strand reaches the parting side where it finally parts, wound
    // round the back of the trunk on either side, never across its front.
    let (target, dir) = if side > 0.0 { (0.0, 1.0) } else { (PI, -1.0) };
    let phase = target - dir * math::TAU * (wind.turns * wind.part_at - wind.offset);
    let mut out = vec![around[start]];
    let mut at = around[start];
    let mut last = (sk.nodes[at].p, zof(sk, at));
    let mut wound = Vec::new();
    for j in start + 1..=m {
        let s = (j - start) as f64 / span;
        if s > part + 1e-9 {
            break;
        }
        let c = sk.nodes[around[j]].p;
        let (a, b) = (
            sk.nodes[around[j - 1]].p,
            sk.nodes[around[(j + 1).min(m)]].p,
        );
        let len = ((b.0 - a.0) * (b.0 - a.0) + (b.1 - a.1) * (b.1 - a.1))
            .sqrt()
            .max(1e-9);
        let n = (-(b.1 - a.1) / len, (b.0 - a.0) / len);
        // Hugs the trunk, a little closer as the trunk tapers.
        let r = wind.radius * (1.0 - 0.45 * s);
        let phi = phase + dir * math::TAU * wind.turns * s;
        let p = (
            c.0 + n.0 * r * math::cos(phi),
            c.1 + n.1 * r * math::cos(phi),
        );
        let z = zof(sk, around[j]) + r * math::sin(phi);
        at = add(sk, p, z, at, lang);
        out.push(at);
        wound.push((at, around[j], phi));
        last = (p, z);
    }
    let free = out.len();
    if peel <= 0.0 {
        sk.wound.push((wound, Vec::new()));
        return (out, free);
    }
    // The free part: out and down first, as if heavy, then up to its top.
    let (p0, z0) = last;
    let l = ((top.0 - p0.0) * (top.0 - p0.0) + (top.1 - p0.1) * (top.1 - p0.1)).sqrt();
    // Heavier, it leans further out and lower before it rises.
    let g = wind.sag.clamp(0.0, 1.0);
    // It never sags into the soil: a strand parting low dips only a little,
    // or its thick wood folds over itself at the turn.
    let above = (crate::style::SOIL_Y - p0.1).max(0.0);
    let p1 = (
        p0.0 + side * l * (0.3 + 0.45 * g),
        p0.1 + (l * (0.05 + 0.45 * g)).min(above / 3.0),
    );
    let p2 = (
        top.0 + side * l * (0.08 + 0.25 * g),
        top.1 + l * (0.4 - 0.15 * g),
    );
    let n = steps_for(l);
    for i in 1..=n {
        let t = i as f64 / n as f64;
        let u = 1.0 - t;
        let x =
            u * u * u * p0.0 + 3.0 * u * u * t * p1.0 + 3.0 * u * t * t * p2.0 + t * t * t * top.0;
        let y =
            u * u * u * p0.1 + 3.0 * u * u * t * p1.1 + 3.0 * u * t * t * p2.1 + t * t * t * top.1;
        at = add(sk, (x, y), z0 * u, at, lang);
        out.push(at);
    }
    sk.wound.push((wound, out[free..].to_vec()));
    (out, free)
}

/// Lays every wound strand on its trunk's surface, now that the wood's
/// girth is known: no gap between them, the strand a little sunk in. The
/// free part follows, less toward its tip, so the parting stays smooth.
pub fn hug(sk: &mut Skeleton) {
    let flow = sk.flows();
    let radius = crate::scene::radius;
    for (wound, free) in std::mem::take(&mut sk.wound) {
        let mut moved = (0.0, 0.0);
        for &(n, t, phi) in &wound {
            let tp = sk.nodes[t].p;
            let back = sk.nodes[t]
                .parent
                .map_or((tp.0, tp.1 + 1.0), |q| sk.nodes[q].p);
            let len = (math::sq(tp.0 - back.0) + math::sq(tp.1 - back.1))
                .sqrt()
                .max(1e-9);
            let normal = (-(tp.1 - back.1) / len, (tp.0 - back.0) / len);
            let off = radius(flow[t]) + radius(flow[n]) * 0.25;
            let p = (
                tp.0 + normal.0 * off * math::cos(phi),
                tp.1 + normal.1 * off * math::cos(phi),
            );
            moved = (p.0 - sk.nodes[n].p.0, p.1 - sk.nodes[n].p.1);
            sk.nodes[n].p = p;
            sk.node_z[n] = zof(sk, t) + off * math::sin(phi);
        }
        let m = free.len().max(1) as f64;
        for (k, &n) in free.iter().enumerate() {
            let f = 1.0 - (k + 1) as f64 / m;
            sk.nodes[n].p.0 += moved.0 * f;
            sk.nodes[n].p.1 += moved.1 * f;
        }
    }
}

/// Bare twigs where a cloud would be in winter: a stem from the nearest of
/// `under` toward `c`, four twigs, and a fork of fine shoots at each.
#[allow(clippy::too_many_arguments, reason = "the shape of a twig fan")]
pub fn winter_fan(
    sk: &mut Skeleton,
    rng: &mut Rng,
    under: &[usize],
    c: P,
    rx: f64,
    tz: f64,
    side: f64,
    lang: &Lang,
    grown: f64,
) {
    let Some(&near) = under.iter().min_by(|a, b| {
        let (pa, pb) = (sk.nodes[**a].p, sk.nodes[**b].p);
        let da = (pa.0 - c.0) * (pa.0 - c.0) + (pa.1 - c.1) * (pa.1 - c.1);
        let db = (pb.0 - c.0) * (pb.0 - c.0) + (pb.1 - c.1) * (pb.1 - c.1);
        da.total_cmp(&db)
    }) else {
        return;
    };
    // A fan still growing in, or breaking back, reaches part of the way from
    // its branch, with fewer twigs; one broken back to nothing is gone.
    if grown < 0.02 {
        return;
    }
    let p = sk.nodes[near].p;
    let c = (p.0 + (c.0 - p.0) * grown, p.1 + (c.1 - p.1) * grown);
    let rx = rx * grown;
    let stem = limb_to(
        sk,
        near,
        c,
        tz,
        0.0,
        1.2,
        1.0,
        rng.range(0.0, math::TAU),
        lang,
    );
    let from = stem[stem.len() / 2];
    let mid = *stem.last().unwrap_or(&from);
    let twigs = [0.0, 0.25, 0.5, 0.75]
        .iter()
        .filter(|&&t| grown > t)
        .count();
    for (dx, dy) in [(-0.75, -0.25), (0.75, -0.3), (-0.3, -0.75), (0.35, -0.7)]
        .into_iter()
        .take(twigs)
    {
        let to = (
            c.0 + side * dx * rx * rng.range(0.8, 1.1),
            c.1 + dy * rx * rng.range(0.8, 1.1),
        );
        let src = if dy < -0.5 { mid } else { from };
        let tw = limb_to(
            sk,
            src,
            to,
            tz + rng.range(-4.0, 4.0),
            0.0,
            0.8,
            1.5,
            rng.range(0.0, math::TAU),
            lang,
        );
        let Some(&tip) = tw.last() else { continue };
        let tp = sk.nodes[tip].p;
        let (ux, uy) = (to.0 - sk.nodes[src].p.0, to.1 - sk.nodes[src].p.1);
        for bend in [-0.5, 0.45] {
            let a = math::atan2(uy, ux) + bend + rng.range(-0.15, 0.15);
            let l = rx * rng.range(0.25, 0.4);
            let q = (tp.0 + math::cos(a) * l, tp.1 + math::sin(a) * l);
            limb_to(sk, tip, q, tz, 0.0, 0.4, 1.0, 0.0, lang);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limb_to_ends_at_its_target() {
        let mut sk = root_only();
        let nodes = limb_to(&mut sk, 0, (300.0, 200.0), 4.0, 6.0, 3.0, 1.5, 0.7, &None);
        let tip = sk.nodes[*nodes.last().unwrap()].p;
        assert!(
            (tip.0 - 300.0).abs() < 0.5 && (tip.1 - 200.0).abs() < 0.5,
            "{tip:?}"
        );
        assert_eq!(sk.node_z.len(), sk.nodes.len());
    }
}
