use std::cmp;

impl Solution {
    pub fn find_median_sorted_arrays(nums1: Vec<i32>, nums2: Vec<i32>) -> f64 {
        let inf = 10000000;

        let n = nums1.len() as i32;
        let m = nums2.len() as i32;

        let mut l = 0;
        let mut h = n;

        loop {
            let t = l + (h - l + 1) / 2;
            let x = (n + m) / 2 - t + (n + m) % 2;

            println!("l={} h={} t={} x={}", l, h, t, x);

            let mut la = -inf;
            if t > 0 {
                if t - 1 < n {
                    la = nums1[t as usize - 1];
                } else {
                    la = inf;
                }
            }

            let mut lb = -inf;
            if x > 0 {
                if x - 1 < m {
                    lb = nums2[x as usize - 1]
                } else {
                    lb = inf;
                }
            }

            let mut ha = inf;
            if t < n {
                if t >= 0 {
                    ha = nums1[t as usize];
                } else {
                    ha = -inf;
                }
            }

            let mut hb = inf;
            if x < m {
                if x >= 0 {
                    hb = nums2[x as usize];
                } else {
                    hb = -inf;
                }
            }

            println!("la={} ha={} lb={} hb={}", la, ha, lb, hb);

            if la > hb {
                h = t - 1;
            } else if lb > ha {
                l = t;
            } else {
                if (n + m) % 2 == 1 {
                    return cmp::max(la, lb) as f64;
                } else {
                    return (cmp::max(la, lb) as f64 + cmp::min(ha, hb) as f64) / 2.0;
                }
            }

            if l > h {
                break;
            }
        }

        return 0.0;
    }
}
