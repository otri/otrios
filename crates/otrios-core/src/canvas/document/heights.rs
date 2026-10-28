// SPDX-License-Identifier: LGPL-3.0-only
//! Fenwick tree for one bounded source order and mutable exact/estimated heights.
//! A new keyed window rebuilds this small index while retained pages keep their
//! logical block identity; height discovery never edits source content.
pub(super) struct Heights {
    values: Vec<f64>,
    tree: Vec<f64>,
}
impl Heights {
    pub fn push(&mut self, height: f64) {
        let n = self.values.len();
        let i = n + 1;
        let low = i & i.wrapping_neg();
        let sum = self.prefix(n) - self.prefix(i - low);
        self.values.push(height);
        self.tree.push(sum + height);
    }
    pub fn new(values: Vec<f64>) -> Self {
        let mut this = Self {
            tree: vec![0.; values.len() + 1],
            values,
        };
        for i in 0..this.values.len() {
            this.add(i, this.values[i]);
        }
        this
    }
    fn add(&mut self, at: usize, delta: f64) {
        let mut i = at + 1;
        while i < self.tree.len() {
            self.tree[i] += delta;
            i += i & i.wrapping_neg();
        }
    }
    pub fn set(&mut self, at: usize, height: f64) {
        let delta = height - self.values[at];
        self.values[at] = height;
        self.add(at, delta);
    }
    pub fn prefix(&self, end: usize) -> f64 {
        let mut n = end;
        let mut sum = 0.;
        while n > 0 {
            sum += self.tree[n];
            n &= n - 1;
        }
        sum
    }
    pub fn total(&self) -> f64 {
        self.prefix(self.values.len())
    }
    /// Block and local y; a boundary belongs to the next block, except at End.
    pub fn locate(&self, y: f64) -> (usize, f64) {
        let mut index = 0;
        let mut sum = 0.;
        let mut bit = self.values.len().next_power_of_two();
        while bit != 0 {
            let next = index + bit;
            if next < self.tree.len() && sum + self.tree[next] <= y {
                sum += self.tree[next];
                index = next;
            }
            bit >>= 1;
        }
        let index = index.min(self.values.len() - 1);
        (index, y - self.prefix(index))
    }
    pub fn bytes(&self) -> usize {
        (self.values.capacity() + self.tree.capacity()) * size_of::<f64>()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn updates_and_large_origins_preserve_local_precision() {
        let mut h = Heights::new(vec![20_000_000., 30., 40.]);
        assert_eq!(h.locate(20_000_000.125), (1, 0.125));
        h.set(0, 40_000_000.);
        assert_eq!(h.total(), 40_000_070.);
        assert_eq!(h.locate(40_000_030.), (2, 0.));
    }
}
