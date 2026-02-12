impl Solution {
    pub fn longest_palindrome(s: String) -> String {
        let ss = s.as_bytes();
        let mut start = 0;
        let mut end = 0;

        for i in 0..ss.len() {
            for l in 0..ss.len() {
                let x = i - l;
                let y = i + l;

                if x < 0 || x >= ss.len() || y >= ss.len() {
                    break;
                }
                if ss[x] != ss[y] {
                    break;
                }
                if y - x > end - start {
                    start = x;
                    end = y;
                }
            }

            for l in 0..ss.len() {
                let x = i - l;
                let y = i + l + 1;

                if x < 0 || x >= ss.len() || y >= ss.len() {
                    break;
                }
                if ss[x] != ss[y] {
                    break;
                }
                if y - x > end - start {
                    start = x;
                    end = y;
                }
            }
        }

        return s[start..(end + 1)].to_string();
    }
}
