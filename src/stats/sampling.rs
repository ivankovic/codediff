/*  This file is part of the CodeDiff code diffing tool.
 *
 *  Copyright (C) 2026 Marko Ivankovic
 *
 *  This program is free software: you can redistribute it and/or modify
 *  it under the terms of the GNU Affero General Public License as published
 *  by the Free Software Foundation, either version 3 of the License, or
 *  (at your option) any later version.
 *
 *  This program is distributed in the hope that it will be useful,
 *  but WITHOUT ANY WARRANTY; without even the implied warranty of
 *  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
 *  GNU Affero General Public License for more details.
 *
 *  You should have received a copy of the GNU Affero General Public License
 *  along with this program. If not, see <https://www.gnu.org/licenses/>.
 */
use rand::Rng;

/// Reservoir sampling (Algorithm R): picks a uniform random sample of `capacity` items from a
/// stream of unknown length in a single pass, without holding the whole stream in memory.
pub struct Reservoir<T> {
    pub items: Vec<T>,
    seen: u64,
}

// Hand-written instead of derived so `T` itself doesn't need to be `Default`.
impl<T> Default for Reservoir<T> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            seen: 0,
        }
    }
}

impl<T> Reservoir<T> {
    pub fn offer(&mut self, item: T, capacity: usize, rng: &mut impl Rng) {
        self.seen += 1;
        if self.items.len() < capacity {
            self.items.push(item);
        } else {
            let j = rng.gen_range(0..self.seen) as usize;
            if j < capacity {
                self.items[j] = item;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    #[test]
    fn reservoir_never_exceeds_capacity() {
        let mut rng = StdRng::seed_from_u64(7);
        let mut reservoir = Reservoir::default();
        for i in 0..1000u32 {
            reservoir.offer(i, 5, &mut rng);
        }
        assert_eq!(reservoir.items.len(), 5);
        assert_eq!(reservoir.seen, 1000);
    }

    #[test]
    fn reservoir_keeps_everything_below_capacity() {
        let mut rng = StdRng::seed_from_u64(7);
        let mut reservoir = Reservoir::default();
        for i in 0..3u32 {
            reservoir.offer(i, 5, &mut rng);
        }
        assert_eq!(reservoir.items, vec![0, 1, 2]);
    }
}
