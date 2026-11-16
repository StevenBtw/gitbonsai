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
    });
    sk.node_z.push(z);
    sk.nodes.len() - 1
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

/// A dropped trunk language: a big bleached snag, a second trunk that died,
/// rising beside the living one.
pub fn dead_trunk(sk: &mut Skeleton, rng: &mut Rng, limb: Limb, len: f64, amount: f64) {
    dead_kinked(
        sk,
        rng,
        &limb,
        len.max(40.0),
        amount.clamp(700.0, 3000.0),
        &[0.5, 0.3, 0.2],
        true,
        5.0,
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
) -> Vec<usize> {
    let (x0, y0) = sk.nodes[from].p;
    let z0 = zof(sk, from);
    let n = steps_for(h);
    let mut at = from;
    let mut out = vec![from];
    for i in 1..=n {
        let u = i as f64 / n as f64;
        let x = x0
            + h * math::sin(lean) * u * (1.0 - 0.42 * u)
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

/// One of a twin trunk's stems from `from`: rises `h`, sets off `gap` to
/// `side` at once, then opens out to `reach` at the top, a V; `amp`, `freq`
/// and `ph` bend it, `zd` drifts it in depth.
#[allow(clippy::too_many_arguments, reason = "the shape of a trunk")]
pub fn twin(
    sk: &mut Skeleton,
    from: usize,
    h: f64,
    side: f64,
    gap: f64,
    reach: f64,
    amp: f64,
    freq: f64,
    ph: f64,
    zd: f64,
    lang: &Lang,
) -> Vec<usize> {
    let (x0, y0) = sk.nodes[from].p;
    let z0 = zof(sk, from);
    let n = steps_for(h);
    let mut at = from;
    let mut out = vec![from];
    let g_end = gap * (1.0 - math::exp(-5.0));
    for i in 1..=n {
        let u = i as f64 / n as f64;
        let x = x0
            + side * (gap * (1.0 - math::exp(-5.0 * u)) + (reach - g_end) * math::pow(u, 1.35))
            + amp * h * (math::sin(ph + freq * math::TAU * u) - math::sin(ph)) * (1.0 - 0.4 * u);
        let y = y0 - h * u;
        let z =
            z0 + zd * u + amp * h * 0.5 * (math::cos(ph + freq * math::TAU * u) - math::cos(ph));
        at = add(sk, (x, y), z, at, lang);
        out.push(at);
    }
    out
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
) {
    let Some(&near) = under.iter().min_by(|a, b| {
        let (pa, pb) = (sk.nodes[**a].p, sk.nodes[**b].p);
        let da = (pa.0 - c.0) * (pa.0 - c.0) + (pa.1 - c.1) * (pa.1 - c.1);
        let db = (pb.0 - c.0) * (pb.0 - c.0) + (pb.1 - c.1) * (pb.1 - c.1);
        da.total_cmp(&db)
    }) else {
        return;
    };
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
    for (dx, dy) in [(-0.75, -0.25), (0.75, -0.3), (-0.3, -0.75), (0.35, -0.7)] {
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
