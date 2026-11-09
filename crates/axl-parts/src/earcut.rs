//! Ear-clipping triangulation of a polygon with holes (a port of the well-known earcut
//! algorithm, without the z-order hash: our polygons have at most a few hundred points).
//!
//! The outer ring must be counter-clockwise and the holes clockwise (y up); [`triangulate`]
//! fixes the orientation itself, so callers need not care.

struct N {
    i: usize,
    x: f64,
    y: f64,
    prev: usize,
    next: usize,
    steiner: bool,
}

struct List {
    n: Vec<N>,
}

/// Twice the signed area of the triangle (mapbox sign: negative for a counter-clockwise turn).
fn area(p: &N, q: &N, r: &N) -> f64 {
    (q.y - p.y) * (r.x - q.x) - (q.x - p.x) * (r.y - q.y)
}

fn eq(a: &N, b: &N) -> bool {
    a.x == b.x && a.y == b.y
}

fn point_in_triangle(a: &N, b: &N, c: &N, p: (f64, f64)) -> bool {
    (c.x - p.0) * (a.y - p.1) >= (a.x - p.0) * (c.y - p.1)
        && (a.x - p.0) * (b.y - p.1) >= (b.x - p.0) * (a.y - p.1)
        && (b.x - p.0) * (c.y - p.1) >= (c.x - p.0) * (b.y - p.1)
}

fn sign(v: f64) -> i32 {
    if v > 0.0 {
        1
    } else if v < 0.0 {
        -1
    } else {
        0
    }
}

fn on_segment(p: &N, q: &N, r: &N) -> bool {
    q.x <= p.x.max(r.x) && q.x >= p.x.min(r.x) && q.y <= p.y.max(r.y) && q.y >= p.y.min(r.y)
}

fn intersects(p1: &N, q1: &N, p2: &N, q2: &N) -> bool {
    let (o1, o2, o3, o4) = (
        sign(area(p1, q1, p2)),
        sign(area(p1, q1, q2)),
        sign(area(p2, q2, p1)),
        sign(area(p2, q2, q1)),
    );
    if o1 != o2 && o3 != o4 {
        return true;
    }
    (o1 == 0 && on_segment(p1, p2, q1))
        || (o2 == 0 && on_segment(p1, q2, q1))
        || (o3 == 0 && on_segment(p2, p1, q2))
        || (o4 == 0 && on_segment(p2, q1, q2))
}

impl List {
    fn nd(&self, i: usize) -> &N {
        &self.n[i]
    }

    fn make(&mut self, i: usize, x: f64, y: f64) -> usize {
        self.n.push(N {
            i,
            x,
            y,
            prev: usize::MAX,
            next: usize::MAX,
            steiner: false,
        });
        self.n.len() - 1
    }

    fn insert_after(&mut self, i: usize, x: f64, y: f64, last: Option<usize>) -> usize {
        let p = self.make(i, x, y);
        match last {
            None => {
                self.n[p].prev = p;
                self.n[p].next = p;
            }
            Some(l) => {
                let nx = self.n[l].next;
                self.n[p].next = nx;
                self.n[p].prev = l;
                self.n[nx].prev = p;
                self.n[l].next = p;
            }
        }
        p
    }

    fn remove(&mut self, p: usize) {
        let (pr, nx) = (self.n[p].prev, self.n[p].next);
        self.n[nx].prev = pr;
        self.n[pr].next = nx;
    }

    fn linked_list(&mut self, data: &[[f64; 2]], start: usize, end: usize, ccw: bool) -> Option<usize> {
        let mut sum = 0.0;
        for i in start..end {
            let j = if i + 1 == end { start } else { i + 1 };
            sum += data[i][0] * data[j][1] - data[j][0] * data[i][1];
        }
        let mut last = None;
        let order: Vec<usize> = if (sum > 0.0) == ccw {
            (start..end).collect()
        } else {
            (start..end).rev().collect()
        };
        for i in order {
            last = Some(self.insert_after(i, data[i][0], data[i][1], last));
        }
        if let Some(l) = last {
            let nx = self.n[l].next;
            if eq(self.nd(l), self.nd(nx)) {
                self.remove(l);
                last = Some(nx);
            }
        }
        last
    }

    fn filter_points(&mut self, start: usize, end: Option<usize>) -> usize {
        let mut end = end.unwrap_or(start);
        let mut p = start;
        loop {
            let mut again = false;
            let (pr, nx) = (self.n[p].prev, self.n[p].next);
            if !self.n[p].steiner
                && (eq(self.nd(p), self.nd(nx)) || area(self.nd(pr), self.nd(p), self.nd(nx)) == 0.0)
            {
                self.remove(p);
                p = pr;
                end = pr;
                if p == nx {
                    break;
                }
                again = true;
            } else {
                p = nx;
            }
            if !again && p == end {
                break;
            }
        }
        end
    }

    fn locally_inside(&self, a: usize, b: usize) -> bool {
        let (an, bn) = (self.nd(a), self.nd(b));
        let (ap, anx) = (self.nd(an.prev), self.nd(an.next));
        if area(ap, an, anx) < 0.0 {
            area(an, bn, anx) >= 0.0 && area(an, ap, bn) >= 0.0
        } else {
            area(an, bn, ap) < 0.0 || area(an, anx, bn) < 0.0
        }
    }

    fn is_ear(&self, ear: usize) -> bool {
        let b = self.nd(ear);
        let (a, c) = (self.nd(b.prev), self.nd(b.next));
        if area(a, b, c) >= 0.0 {
            return false;
        }
        let mut p = c.next;
        while p != b.prev {
            let pn = self.nd(p);
            if !(a.x == pn.x && a.y == pn.y)
                && point_in_triangle(a, b, c, (pn.x, pn.y))
                && area(self.nd(pn.prev), pn, self.nd(pn.next)) >= 0.0
            {
                return false;
            }
            p = pn.next;
        }
        true
    }

    fn earcut(&mut self, ear: usize, out: &mut Vec<usize>, pass: u32) {
        let mut ear = ear;
        let mut stop = ear;
        let mut guard = 0usize;
        while self.n[ear].prev != self.n[ear].next {
            guard += 1;
            if guard > 4 * self.n.len() + 64 {
                return;
            }
            let (prev, next) = (self.n[ear].prev, self.n[ear].next);
            if self.is_ear(ear) {
                out.extend([self.n[prev].i, self.n[ear].i, self.n[next].i]);
                self.remove(ear);
                ear = self.n[next].next;
                stop = ear;
                continue;
            }
            ear = next;
            if ear == stop {
                match pass {
                    0 => {
                        let e = self.filter_points(ear, None);
                        self.earcut(e, out, 1);
                    }
                    1 => {
                        let e = self.filter_points(ear, None);
                        let e = self.cure_local_intersections(e, out);
                        self.earcut(e, out, 2);
                    }
                    2 => self.split_earcut(ear, out),
                    _ => {}
                }
                return;
            }
        }
    }

    fn cure_local_intersections(&mut self, start: usize, out: &mut Vec<usize>) -> usize {
        let mut start = start;
        let mut p = start;
        loop {
            let a = self.n[p].prev;
            let pn = self.n[p].next;
            let b = self.n[pn].next;
            if !eq(self.nd(a), self.nd(b))
                && intersects(self.nd(a), self.nd(p), self.nd(pn), self.nd(b))
                && self.locally_inside(a, b)
                && self.locally_inside(b, a)
            {
                out.extend([self.n[a].i, self.n[p].i, self.n[b].i]);
                self.remove(p);
                self.remove(pn);
                p = b;
                start = b;
            }
            p = self.n[p].next;
            if p == start {
                break;
            }
        }
        self.filter_points(p, None)
    }

    fn split_earcut(&mut self, start: usize, out: &mut Vec<usize>) {
        let mut a = start;
        loop {
            let mut b = self.n[self.n[a].next].next;
            while b != self.n[a].prev {
                if self.n[a].i != self.n[b].i && self.is_valid_diagonal(a, b) {
                    let c = self.split_polygon(a, b);
                    let an = self.n[a].next;
                    let a2 = self.filter_points(a, Some(an));
                    let cn = self.n[c].next;
                    let c2 = self.filter_points(c, Some(cn));
                    self.earcut(a2, out, 0);
                    self.earcut(c2, out, 0);
                    return;
                }
                b = self.n[b].next;
            }
            a = self.n[a].next;
            if a == start {
                break;
            }
        }
    }

    fn is_valid_diagonal(&self, a: usize, b: usize) -> bool {
        let (an, bn) = (self.nd(a), self.nd(b));
        self.nd(an.next).i != bn.i
            && self.nd(an.prev).i != bn.i
            && !self.intersects_polygon(a, b)
            && ((self.locally_inside(a, b)
                && self.locally_inside(b, a)
                && self.middle_inside(a, b)
                && (area(self.nd(an.prev), an, self.nd(bn.prev)) != 0.0
                    || area(an, self.nd(bn.prev), bn) != 0.0))
                || (eq(an, bn)
                    && area(self.nd(an.prev), an, self.nd(an.next)) > 0.0
                    && area(self.nd(bn.prev), bn, self.nd(bn.next)) > 0.0))
    }

    fn intersects_polygon(&self, a: usize, b: usize) -> bool {
        let (an, bn) = (self.nd(a), self.nd(b));
        let mut p = a;
        loop {
            let pn = self.nd(p);
            let nx = self.nd(pn.next);
            if pn.i != an.i
                && nx.i != an.i
                && pn.i != bn.i
                && nx.i != bn.i
                && intersects(pn, nx, an, bn)
            {
                return true;
            }
            p = pn.next;
            if p == a {
                return false;
            }
        }
    }

    fn middle_inside(&self, a: usize, b: usize) -> bool {
        let (an, bn) = (self.nd(a), self.nd(b));
        let (mx, my) = ((an.x + bn.x) / 2.0, (an.y + bn.y) / 2.0);
        let mut inside = false;
        let mut p = a;
        loop {
            let pn = self.nd(p);
            let nx = self.nd(pn.next);
            if ((pn.y > my) != (nx.y > my))
                && nx.y != pn.y
                && mx < (nx.x - pn.x) * (my - pn.y) / (nx.y - pn.y) + pn.x
            {
                inside = !inside;
            }
            p = pn.next;
            if p == a {
                return inside;
            }
        }
    }

    fn split_polygon(&mut self, a: usize, b: usize) -> usize {
        let (ai, ax, ay) = (self.n[a].i, self.n[a].x, self.n[a].y);
        let (bi, bx, by) = (self.n[b].i, self.n[b].x, self.n[b].y);
        let a2 = self.make(ai, ax, ay);
        let b2 = self.make(bi, bx, by);
        let an = self.n[a].next;
        let bp = self.n[b].prev;
        self.n[a].next = b;
        self.n[b].prev = a;
        self.n[a2].next = an;
        self.n[an].prev = a2;
        self.n[b2].next = a2;
        self.n[a2].prev = b2;
        self.n[bp].next = b2;
        self.n[b2].prev = bp;
        b2
    }

    fn sector_contains_sector(&self, m: usize, p: usize) -> bool {
        let (mn, pn) = (self.nd(m), self.nd(p));
        area(self.nd(mn.prev), mn, self.nd(pn.prev)) < 0.0
            && area(self.nd(pn.next), mn, self.nd(mn.next)) < 0.0
    }

    fn find_hole_bridge(&self, hole: usize, outer: usize) -> Option<usize> {
        let h = self.nd(hole);
        let (hx, hy) = (h.x, h.y);
        let mut qx = f64::NEG_INFINITY;
        let mut m: Option<usize> = None;
        let mut p = outer;
        loop {
            let pn = self.nd(p);
            let nx = self.nd(pn.next);
            if eq(h, pn) {
                return Some(p);
            }
            if hy <= pn.y && hy >= nx.y && nx.y != pn.y {
                let x = pn.x + (hy - pn.y) * (nx.x - pn.x) / (nx.y - pn.y);
                if x <= hx && x > qx {
                    qx = x;
                    m = Some(if pn.x < nx.x { p } else { pn.next });
                    if x == hx {
                        return m;
                    }
                }
            }
            p = pn.next;
            if p == outer {
                break;
            }
        }
        let m = m?;
        let stop = m;
        let (mx, my) = (self.nd(m).x, self.nd(m).y);
        let mut tan_min = f64::INFINITY;
        let mut best = m;
        let mut p = m;
        loop {
            let pn = self.nd(p);
            if hx >= pn.x && pn.x >= mx && hx != pn.x {
                let tri = if hy < my {
                    ((hx, hy), (mx, my), (qx, hy))
                } else {
                    ((qx, hy), (mx, my), (hx, hy))
                };
                let (a, b, c) = (
                    N { i: 0, x: tri.0.0, y: tri.0.1, prev: 0, next: 0, steiner: false },
                    N { i: 0, x: tri.1.0, y: tri.1.1, prev: 0, next: 0, steiner: false },
                    N { i: 0, x: tri.2.0, y: tri.2.1, prev: 0, next: 0, steiner: false },
                );
                if point_in_triangle(&a, &b, &c, (pn.x, pn.y)) {
                    let tn = (hy - pn.y).abs() / (hx - pn.x);
                    if self.locally_inside(p, hole)
                        && (tn < tan_min
                            || (tn == tan_min
                                && (pn.x > self.nd(best).x
                                    || (pn.x == self.nd(best).x
                                        && self.sector_contains_sector(best, p)))))
                    {
                        best = p;
                        tan_min = tn;
                    }
                }
            }
            p = pn.next;
            if p == stop {
                break;
            }
        }
        Some(best)
    }

    fn eliminate_holes(
        &mut self,
        data: &[[f64; 2]],
        hole_starts: &[usize],
        mut outer: usize,
    ) -> usize {
        let mut queue: Vec<usize> = Vec::new();
        for (k, &start) in hole_starts.iter().enumerate() {
            let end = hole_starts.get(k + 1).copied().unwrap_or(data.len());
            if let Some(list) = self.linked_list(data, start, end, false) {
                if self.n[list].next == list {
                    self.n[list].steiner = true;
                }
                // Leftmost point of the hole.
                let mut leftmost = list;
                let mut p = self.n[list].next;
                while p != list {
                    let (a, b) = (self.nd(p), self.nd(leftmost));
                    if a.x < b.x || (a.x == b.x && a.y < b.y) {
                        leftmost = p;
                    }
                    p = self.n[p].next;
                }
                queue.push(leftmost);
            }
        }
        queue.sort_by(|&a, &b| self.n[a].x.total_cmp(&self.n[b].x));
        for hole in queue {
            if let Some(bridge) = self.find_hole_bridge(hole, outer) {
                let rev = self.split_polygon(bridge, hole);
                let rn = self.n[rev].next;
                self.filter_points(rev, Some(rn));
                let bn = self.n[bridge].next;
                outer = self.filter_points(bridge, Some(bn));
            }
        }
        outer
    }
}

/// Triangulate `data` (the outer ring first, then each hole). `hole_starts` are the indices
/// in `data` where each hole begins. Returns triangle corner indices into `data`.
pub fn triangulate(data: &[[f64; 2]], hole_starts: &[usize]) -> Vec<usize> {
    let outer_len = hole_starts.first().copied().unwrap_or(data.len());
    let mut list = List {
        n: Vec::with_capacity(data.len() * 2 + 8),
    };
    let mut out = Vec::with_capacity(data.len() * 3);
    let Some(mut outer) = list.linked_list(data, 0, outer_len, true) else {
        return out;
    };
    if list.n[outer].next == list.n[outer].prev {
        return out;
    }
    if !hole_starts.is_empty() {
        outer = list.eliminate_holes(data, hole_starts, outer);
    }
    let outer = list.filter_points(outer, None);
    list.earcut(outer, &mut out, 0);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tri_area(data: &[[f64; 2]], t: &[usize]) -> f64 {
        t.chunks(3)
            .map(|c| {
                let (a, b, d) = (data[c[0]], data[c[1]], data[c[2]]);
                0.5 * ((b[0] - a[0]) * (d[1] - a[1]) - (b[1] - a[1]) * (d[0] - a[0])).abs()
            })
            .sum()
    }

    fn circle(cx: f64, cy: f64, r: f64, n: usize, cw: bool) -> Vec<[f64; 2]> {
        (0..n)
            .map(|i| {
                let a = std::f64::consts::TAU * i as f64 / n as f64 * if cw { -1.0 } else { 1.0 };
                [cx + r * a.cos(), cy + r * a.sin()]
            })
            .collect()
    }

    #[test]
    fn square_makes_two_triangles() {
        let d = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        let t = triangulate(&d, &[]);
        assert_eq!(t.len(), 6);
        assert!((tri_area(&d, &t) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn clockwise_input_is_accepted() {
        let d = [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]];
        let t = triangulate(&d, &[]);
        assert!((tri_area(&d, &t) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn concave_polygon_area_is_preserved() {
        // An L shape of area 3.
        let d = [[0.0, 0.0], [2.0, 0.0], [2.0, 1.0], [1.0, 1.0], [1.0, 2.0], [0.0, 2.0]];
        let t = triangulate(&d, &[]);
        assert!((tri_area(&d, &t) - 3.0).abs() < 1e-12);
    }

    #[test]
    fn rectangle_with_many_holes_has_the_right_area() {
        let mut d: Vec<[f64; 2]> = vec![[-3.6, -3.6], [60.0, -3.6], [60.0, 3.6], [-3.6, 3.6]];
        let mut starts = Vec::new();
        for k in 0..7 {
            starts.push(d.len());
            d.extend(circle(8.0 * k as f64, 0.0, 2.4, 32, true));
        }
        let t = triangulate(&d, &starts);
        let hole_area: f64 = 7.0 * 0.5 * 32.0 * 2.4 * 2.4 * (std::f64::consts::TAU / 32.0).sin();
        let want = 63.6 * 7.2 - hole_area;
        assert!((tri_area(&d, &t) - want).abs() < 1e-6, "{} vs {want}", tri_area(&d, &t));
    }

    #[test]
    fn ring_with_a_cross_hole() {
        let mut d = circle(0.0, 0.0, 10.0, 48, false);
        let start = d.len();
        // A plus shape, clockwise.
        let plus = [
            [0.9, 2.4], [-0.9, 2.4], [-0.9, 0.9], [-2.4, 0.9], [-2.4, -0.9], [-0.9, -0.9],
            [-0.9, -2.4], [0.9, -2.4], [0.9, -0.9], [2.4, -0.9], [2.4, 0.9], [0.9, 0.9],
        ];
        d.extend(plus);
        let t = triangulate(&d, &[start]);
        let outer = 0.5 * 48.0 * 100.0 * (std::f64::consts::TAU / 48.0).sin();
        let want = outer - 14.04;
        assert!((tri_area(&d, &t) - want).abs() < 1e-6);
    }
}
