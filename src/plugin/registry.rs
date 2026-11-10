#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Order {
    Priority(i32),
    Before(String),
    After(String),
}

impl Default for Order {
    fn default() -> Self {
        Self::Priority(0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Contribution {
    pub(crate) plugin_id: String,
    pub(crate) key: String,
    pub(crate) value: String,
    pub(crate) order: Order,
}

pub(crate) fn sort(items: &mut [Contribution]) {
    let mut incoming = vec![0usize; items.len()];
    let mut edges = vec![Vec::new(); items.len()];
    for (index, item) in items.iter().enumerate() {
        let (target, before) = match &item.order {
            Order::Before(target) => (target.as_str(), true),
            Order::After(target) => (target.as_str(), false),
            Order::Priority(_) => continue,
        };
        for (other_index, other) in items.iter().enumerate() {
            if index == other_index || other.plugin_id != target {
                continue;
            }
            let (from, to) = if before {
                (index, other_index)
            } else {
                (other_index, index)
            };
            edges[from].push(to);
            incoming[to] += 1;
        }
    }

    let mut ready = (0..items.len())
        .filter(|index| incoming[*index] == 0)
        .collect::<Vec<_>>();
    let mut result = Vec::with_capacity(items.len());
    let mut visited = HashSet::new();
    while !ready.is_empty() {
        ready.sort_by_key(|index| {
            let item = &items[*index];
            (priority(&item.order), item.plugin_id.as_str(), *index)
        });
        let index = ready.remove(0);
        visited.insert(index);
        result.push(items[index].clone());
        for target in &edges[index] {
            incoming[*target] -= 1;
            if incoming[*target] == 0 {
                ready.push(*target);
            }
        }
    }

    if result.len() != items.len() {
        let mut remaining = (0..items.len())
            .filter(|index| !visited.contains(index))
            .map(|index| items[index].clone())
            .collect::<Vec<_>>();
        remaining.sort_by_key(|item| (priority(&item.order), item.plugin_id.clone()));
        result.extend(remaining);
    }
    items.clone_from_slice(&result);
}

fn priority(order: &Order) -> i32 {
    match order {
        Order::Priority(priority) => *priority,
        Order::Before(_) | Order::After(_) => 0,
    }
}
use std::collections::HashSet;
