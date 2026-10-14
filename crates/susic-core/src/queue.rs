use rand::seq::SliceRandom;
use rand::Rng;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum RepeatMode {
    #[default]
    Off,
    All,
    One,
}

/// A play queue holding track ids. In shuffle mode a separate play order is kept
/// so toggling shuffle never loses the user's original ordering, and the
/// shuffled order is a full permutation (no repeats until every track played).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PlayQueue {
    /// Tracks in user order.
    pub items: Vec<i64>,
    /// Index into `items` of the current track.
    pub current: Option<usize>,
    pub shuffle: bool,
    pub repeat: RepeatMode,
    /// Permutation of item indices used when shuffle is on.
    order: Vec<usize>,
    /// Position inside `order`.
    pos: usize,
}

impl PlayQueue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&mut self, items: Vec<i64>, start: usize) {
        self.items = items;
        self.current = if self.items.is_empty() { None } else { Some(start.min(self.items.len() - 1)) };
        self.rebuild_order();
    }

    pub fn current_id(&self) -> Option<i64> {
        self.current.and_then(|i| self.items.get(i).copied())
    }

    fn rebuild_order(&mut self) {
        let n = self.items.len();
        self.order = (0..n).collect();
        self.pos = 0;
        if self.shuffle {
            let mut rng = rand::thread_rng();
            self.order.shuffle(&mut rng);
            if let Some(c) = self.current {
                // Current track is first in the shuffled order.
                if let Some(p) = self.order.iter().position(|&x| x == c) {
                    self.order.swap(0, p);
                }
            }
        } else if let Some(c) = self.current {
            self.pos = c;
        }
    }

    pub fn set_shuffle(&mut self, on: bool) {
        self.shuffle = on;
        self.rebuild_order();
    }

    pub fn set_repeat(&mut self, r: RepeatMode) {
        self.repeat = r;
    }

    fn sync_pos(&mut self) {
        if let Some(c) = self.current {
            if let Some(p) = self.order.iter().position(|&x| x == c) {
                self.pos = p;
            }
        }
    }

    /// Advance. `auto` is true when a track ended naturally (repeat-one applies).
    pub fn next(&mut self, auto: bool) -> Option<i64> {
        if self.items.is_empty() {
            return None;
        }
        if auto && self.repeat == RepeatMode::One {
            return self.current_id();
        }
        self.sync_pos();
        let n = self.order.len();
        if self.pos + 1 < n {
            self.pos += 1;
        } else if self.repeat == RepeatMode::All || (!auto && self.repeat == RepeatMode::One) {
            if self.shuffle {
                let last = self.order[n - 1];
                let mut rng = rand::thread_rng();
                self.order.shuffle(&mut rng);
                // Avoid replaying the track we just finished first.
                if n > 1 && self.order[0] == last {
                    let j = rng.gen_range(1..n);
                    self.order.swap(0, j);
                }
            }
            self.pos = 0;
        } else {
            return None;
        }
        self.current = Some(self.order[self.pos]);
        self.current_id()
    }

    pub fn prev(&mut self) -> Option<i64> {
        if self.items.is_empty() {
            return None;
        }
        self.sync_pos();
        if self.pos > 0 {
            self.pos -= 1;
        } else if self.repeat == RepeatMode::All {
            self.pos = self.order.len() - 1;
        }
        self.current = Some(self.order[self.pos]);
        self.current_id()
    }

    pub fn jump_to(&mut self, index: usize) -> Option<i64> {
        if index >= self.items.len() {
            return None;
        }
        self.current = Some(index);
        self.sync_pos();
        self.current_id()
    }

    /// Insert directly after the current track (Play Next).
    pub fn play_next(&mut self, ids: Vec<i64>) {
        let at = self.current.map(|c| c + 1).unwrap_or(self.items.len());
        self.insert_at(at, ids);
    }

    pub fn append(&mut self, ids: Vec<i64>) {
        let at = self.items.len();
        self.insert_at(at, ids);
    }

    fn insert_at(&mut self, at: usize, ids: Vec<i64>) {
        if ids.is_empty() {
            return;
        }
        let count = ids.len();
        let cur_id = self.current_id();
        let was_empty = self.items.is_empty();
        self.items.splice(at..at, ids);
        if was_empty {
            self.current = Some(0);
        } else if let Some(c) = self.current {
            if at <= c {
                self.current = Some(c + count);
            }
        }
        let _ = cur_id;
        // Keep existing shuffle order: shift indices and slot new ones after pos.
        for o in self.order.iter_mut() {
            if *o >= at {
                *o += count;
            }
        }
        if self.shuffle {
            let insert_pos = (self.pos + 1).min(self.order.len());
            let new: Vec<usize> = (at..at + count).collect();
            self.order.splice(insert_pos..insert_pos, new);
            if was_empty {
                self.pos = 0;
            }
        } else {
            self.order = (0..self.items.len()).collect();
            self.pos = self.current.unwrap_or(0);
        }
    }

    pub fn remove(&mut self, index: usize) {
        if index >= self.items.len() {
            return;
        }
        self.items.remove(index);
        match self.current {
            Some(c) if c == index => {
                self.current = if self.items.is_empty() { None } else { Some(index.min(self.items.len() - 1)) };
            }
            Some(c) if c > index => self.current = Some(c - 1),
            _ => {}
        }
        self.order.retain(|&o| o != index);
        for o in self.order.iter_mut() {
            if *o > index {
                *o -= 1;
            }
        }
        self.sync_pos();
    }

    pub fn clear_upcoming(&mut self) {
        match self.current {
            Some(c) => {
                self.items.truncate(c + 1);
                self.rebuild_order();
            }
            None => self.clear(),
        }
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.current = None;
        self.order.clear();
        self.pos = 0;
    }

    pub fn move_item(&mut self, from: usize, to: usize) {
        if from >= self.items.len() || to >= self.items.len() || from == to {
            return;
        }
        let cur = self.current_id();
        let item = self.items.remove(from);
        self.items.insert(to, item);
        // Recover the current index by identity of position tracking.
        if let Some(c) = self.current {
            self.current = Some(if c == from {
                to
            } else if from < c && to >= c {
                c - 1
            } else if from > c && to <= c {
                c + 1
            } else {
                c
            });
        }
        let _ = cur;
        self.rebuild_order();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequential_next_prev() {
        let mut q = PlayQueue::new();
        q.set(vec![10, 20, 30], 0);
        assert_eq!(q.next(false), Some(20));
        assert_eq!(q.next(false), Some(30));
        assert_eq!(q.next(true), None);
        assert_eq!(q.prev(), Some(20));
    }

    #[test]
    fn repeat_modes() {
        let mut q = PlayQueue::new();
        q.set(vec![1, 2], 1);
        q.set_repeat(RepeatMode::One);
        assert_eq!(q.next(true), Some(2));
        q.set_repeat(RepeatMode::All);
        assert_eq!(q.next(true), Some(1));
    }

    #[test]
    fn shuffle_plays_every_track_once() {
        let mut q = PlayQueue::new();
        q.set((0..50).collect(), 7);
        q.set_shuffle(true);
        let mut seen = vec![q.current_id().unwrap()];
        while let Some(id) = q.next(true) {
            seen.push(id);
        }
        assert_eq!(seen[0], 7);
        seen.sort();
        assert_eq!(seen, (0..50).collect::<Vec<_>>());
    }

    #[test]
    fn shuffle_repeat_all_does_not_immediately_repeat() {
        let mut q = PlayQueue::new();
        q.set((0..5).collect(), 0);
        q.set_shuffle(true);
        q.set_repeat(RepeatMode::All);
        let mut last = q.current_id().unwrap();
        for _ in 0..200 {
            let n = q.next(true).unwrap();
            assert_ne!(n, last);
            last = n;
        }
    }

    #[test]
    fn play_next_and_remove() {
        let mut q = PlayQueue::new();
        q.set(vec![1, 2, 3], 0);
        q.play_next(vec![9]);
        assert_eq!(q.items, vec![1, 9, 2, 3]);
        assert_eq!(q.next(false), Some(9));
        q.remove(0);
        assert_eq!(q.current_id(), Some(9));
        assert_eq!(q.items, vec![9, 2, 3]);
    }

    #[test]
    fn move_keeps_current() {
        let mut q = PlayQueue::new();
        q.set(vec![1, 2, 3, 4], 1);
        q.move_item(3, 0);
        assert_eq!(q.items, vec![4, 1, 2, 3]);
        assert_eq!(q.current_id(), Some(2));
        q.move_item(1, 3);
        assert_eq!(q.current_id(), Some(2));
    }
}
