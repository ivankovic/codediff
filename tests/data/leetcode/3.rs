use std::collections::HashMap;

impl Solution {
    pub fn length_of_longest_substring(s: String) -> i32 {
        let mut longest = 0;

        let mut last_ok = 0;
        let mut last_char = HashMap::new();

        for (i, u) in s.as_bytes().iter().enumerate() {
            let ii = i as i32;

            let l = ii - last_ok;
            if l > longest {
                longest = l;
            }

            if let Some(old) = last_char.insert(u, ii) {
                if (old + 1) > last_ok {
                    last_ok = (old + 1)
                }
            }
        }

        if s.len() as i32 - last_ok > longest {
            longest = s.len() as i32 - last_ok;
        }

        return longest;
    }
}
