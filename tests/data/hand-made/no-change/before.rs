// Definition for singly-linked list.
// #[derive(PartialEq, Eq, Clone, Debug)]
// pub struct ListNode {
//   pub val: i32,
//   pub next: Option<Box<ListNode>>
// }
//
// impl ListNode {
//   #[inline]
//   fn new(val: i32) -> Self {
//     ListNode {
//       next: None,
//       val
//     }
//   }
// }
impl Solution {
    pub fn add_two_numbers(
        mut l1: Option<Box<ListNode>>,
        mut l2: Option<Box<ListNode>>,
    ) -> Option<Box<ListNode>> {
        let mut result = Box::new(ListNode::new(0));
        let mut tail = &mut result;
        let mut carry = 0;

        while l1.is_some() || l2.is_some() || carry > 0 {
            let x = l1.as_ref().map(|n| n.val).unwrap_or(0);
            let y = l2.as_ref().map(|n| n.val).unwrap_or(0);

            let s = x + y + carry;
            carry = s / 10;

            tail.next = Some(Box::new(ListNode::new(s % 10)));
            tail = tail.next.as_mut().unwrap();

            l1 = match l1 {
                Some(node) => node.next,
                None => None,
            };

            l2 = match l2 {
                Some(node) => node.next,
                None => None,
            };
        }

        return result.next;
    }
}
