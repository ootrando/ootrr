pub const ROOT: u32 = u32::MAX;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    pub bit: u32,
    pub left: u16,
    pub right: u16,
    pub key: Vec<u8>,
}

fn bit(key: &[u8], b: u32) -> bool {
    let i: usize = (b >> 3) as usize;
    if i >= key.len() {
        return false;
    }
    (key[key.len() - 1 - i] >> (b & 7)) & 1 == 1
}

fn first_diff(a: &[u8], b: &[u8]) -> u32 {
    let n: u32 = (a.len().max(b.len()) * 8) as u32;
    (0..n).find(|x| bit(a, *x) != bit(b, *x)).unwrap_or(n)
}

fn deeper(nodes: &[Node], prev: usize, cur: usize) -> bool {
    let (p, c): (u32, u32) = (nodes[prev].bit, nodes[cur].bit);
    c != ROOT && (p == ROOT || c > p)
}

fn step(nodes: &[Node], cur: usize, key: &[u8]) -> usize {
    if bit(key, nodes[cur].bit) { nodes[cur].right as usize } else { nodes[cur].left as usize }
}

pub fn build(names: &[&str]) -> Vec<Node> {
    let mut nodes: Vec<Node> = vec![Node { bit: ROOT, left: 0, right: 0, key: Vec::new() }];
    for (n, name) in names.iter().enumerate() {
        let i: usize = n + 1;
        let k: &[u8] = name.as_bytes();
        let (mut prev, mut cur): (usize, usize) = (0, nodes[0].left as usize);
        while deeper(&nodes, prev, cur) {
            prev = cur;
            cur = step(&nodes, cur, k);
        }
        let d: u32 = first_diff(k, &nodes[cur].key);
        let (mut prev, mut cur): (usize, usize) = (0, nodes[0].left as usize);
        while deeper(&nodes, prev, cur) && nodes[cur].bit <= d {
            prev = cur;
            cur = step(&nodes, cur, k);
        }
        let (left, right): (u16, u16) = if bit(k, d) { (cur as u16, i as u16) } else { (i as u16, cur as u16) };
        nodes.push(Node { bit: d, left, right, key: k.to_vec() });
        if prev == 0 {
            nodes[0].left = i as u16;
        } else if bit(k, nodes[prev].bit) {
            nodes[prev].right = i as u16;
        } else {
            nodes[prev].left = i as u16;
        }
    }
    nodes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shape(names: &[&str]) -> Vec<(u32, u16, u16)> {
        build(names).iter().map(|n| (n.bit, n.left, n.right)).collect()
    }

    #[test]
    fn the_trees_match_the_game_dictionaries() {
        assert_eq!(shape(&["Beard__Mt_Item_Bombchu_Mtl", "Body__Mt_Item_Bombchu"]), vec![(ROOT, 2, 0), (2, 0, 1), (0, 1, 2)]);
        assert_eq!(shape(&["Mt_Item_Bombchu", "Mt_Item_Bombchu_Mtl"]), vec![(ROOT, 1, 0), (0, 2, 1), (2, 0, 2)]);
        assert_eq!(
            shape(&["Root", "Skl_Root", "Roll", "Effect_0", "Front_Tyre", "Rear_Tyre", "Tail"]),
            vec![(ROOT, 5, 0), (2, 4, 3), (0x20, 1, 2), (3, 2, 7), (4, 0, 4), (0, 1, 6), (0x29, 5, 6), (8, 3, 7)]
        );
        assert_eq!(
            shape(&["Bag_Bottom__Mt_Item_BombBag_Small", "Bomb_2__Mt_Obj_Bomb", "Metal_1_1__Mt_Obj_Bomb_Metal"]),
            vec![(ROOT, 2, 0), (2, 0, 3), (1, 1, 2), (8, 1, 3)]
        );
        assert_eq!(shape(&["Root", "Bomb", "Skl_Root", "Bomb_00", "Bomb_01"]), vec![(ROOT, 5, 0), (2, 4, 3), (1, 1, 2), (0x20, 1, 3), (4, 0, 4), (0, 2, 5)]);
    }

    fn find(nodes: &[Node], key: &str) -> Option<usize> {
        let k: &[u8] = key.as_bytes();
        let (mut prev, mut cur): (usize, usize) = (0, nodes[0].left as usize);
        while deeper(nodes, prev, cur) {
            prev = cur;
            cur = step(nodes, cur, k);
        }
        (nodes[cur].key == k).then_some(cur)
    }

    #[test]
    fn every_name_is_found_at_its_own_index() {
        let names: Vec<String> = (0..40).map(|i| format!("Shape_{i}__Mt_{}", i * 7 % 13)).collect();
        let refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
        let nodes: Vec<Node> = build(&refs);
        for (i, n) in refs.iter().enumerate() {
            assert_eq!(find(&nodes, n), Some(i + 1), "{n}");
        }
        assert_eq!(find(&nodes, "Missing"), None);
    }
}
