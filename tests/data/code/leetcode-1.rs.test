use std::collections::HashMap;

impl Solution {
    pub fn two_sum(nums: Vec<i32>, target: i32) -> Vec<i32> {
        let mut indices = HashMap::new();

        for (i, v) in nums.iter().enumerate() {
            indices.insert(v, i);
        }

        for (i, v) in nums.iter().enumerate() {
            let x = target - v;
            let t = indices.get_key_value(&x);

            match t {
                Some((_, y)) => {
                    if *y != i {
                        let mut r = Vec::new();
                        r.push(i as i32);
                        r.push(*y as i32);
                        return r;
                    }
                }
                None => (),
            }
        }

        return Vec::new();
    }
}
