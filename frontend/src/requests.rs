use crate::generated::{API_VERSION, Command, Event, Notice, Request};
use std::collections::BTreeMap;

/// UI destinations, not backend endpoints. Replaced reads cannot change a new view.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Target {
    Bootstrap,
    Main,
    Queue,
    Lyrics,
    LyricMatches,
    Artwork(String),
    Mode,
    Volume,
    Seek,
    Create,
    Favorite(String),
    MainEdit,
    QueueEdit,
    Mutation,
}

impl Target {
    pub fn is_control(&self) -> bool {
        matches!(
            self,
            Self::Mutation
                | Self::MainEdit
                | Self::QueueEdit
                | Self::Mode
                | Self::Volume
                | Self::Seek
                | Self::Create
                | Self::Favorite(_)
        )
    }
}

pub struct Response {
    pub target: Option<Target>,
    pub sequence: u64,
    pub notice: Notice,
}

#[derive(Default)]
pub struct Requests {
    next_id: u64,
    pending: BTreeMap<u64, Target>,
    latest: BTreeMap<Target, u64>,
}

impl Requests {
    pub fn prepare(&mut self, command: Command, target: Target) -> Result<Request, &'static str> {
        if self.pending.len() >= 16 {
            return Err("처리 중입니다. 잠시 후 다시 시도해주세요");
        }
        let id = self
            .next_id
            .checked_add(1)
            .ok_or("요청 번호를 모두 사용했습니다")?;
        self.next_id = id;
        if target != Target::Mutation {
            self.latest.insert(target.clone(), id);
        }
        self.pending.insert(id, target);
        Ok(Request {
            version: API_VERSION,
            id,
            command,
        })
    }

    /// Keep accepted requests counted until their reply, even when no longer visible.
    pub fn invalidate(&mut self, target: &Target) {
        self.latest.remove(target);
    }

    pub fn pending_for(&self, target: &Target) -> Vec<u64> {
        self.pending
            .iter()
            .filter_map(|(&id, pending)| (pending == target).then_some(id))
            .collect()
    }

    /// Only for a request the transport explicitly rejected before admission.
    pub fn rejected(&mut self, id: u64) {
        if let Some(target) = self.pending.remove(&id)
            && self.latest.get(&target) == Some(&id)
        {
            self.latest.remove(&target);
        }
    }

    pub fn complete(&mut self, event: Event) -> Option<Response> {
        let target = if let Some(id) = event.id {
            let target = self.pending.remove(&id)?;
            if target == Target::Mutation || self.latest.get(&target) == Some(&id) {
                self.latest.remove(&target);
                Some(target)
            } else if target.is_control()
                && matches!(
                    event.event,
                    Notice::Store(_)
                        | Notice::CollectionCreated(_)
                        | Notice::Player(_)
                        | Notice::Volume(_)
                )
            {
                // Merge confirmed state without settling a newer UI intention.
                None
            } else {
                return None;
            }
        } else {
            None
        };
        Some(Response {
            target,
            sequence: event.sequence,
            notice: event.event,
        })
    }

    pub fn is_idle(&self) -> bool {
        self.pending.is_empty()
    }

    pub fn count(&self) -> usize {
        self.pending.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generated::{Ack, Empty};

    fn event(id: Option<u64>, sequence: u64) -> Event {
        Event {
            version: API_VERSION,
            id,
            sequence,
            event: Notice::Ack(Ack {
                message: "ok".into(),
            }),
        }
    }

    #[test]
    fn superseded_reads_cannot_update_new_views() {
        let mut requests = Requests::default();
        let old = requests
            .prepare(Command::Snapshot(Empty {}), Target::Main)
            .unwrap();
        let new = requests
            .prepare(Command::Snapshot(Empty {}), Target::Main)
            .unwrap();
        let mutation = requests
            .prepare(Command::Snapshot(Empty {}), Target::Mutation)
            .unwrap();
        assert!(requests.complete(event(Some(new.id), 8)).is_some());
        assert!(requests.complete(event(Some(old.id), 9)).is_none());
        assert!(requests.complete(event(Some(mutation.id), 3)).is_some());
        assert!(requests.complete(event(Some(mutation.id), 10)).is_none());
        // Coalesced unsolicited streams can interleave with older correlated replies.
        assert!(requests.complete(event(None, 2)).is_some());
        assert!(requests.is_idle());
    }

    #[test]
    fn abandoned_views_stay_bounded_until_settled() {
        let mut requests = Requests::default();
        for _ in 0..16 {
            requests
                .prepare(Command::Snapshot(Empty {}), Target::Main)
                .unwrap();
        }
        assert!(
            requests
                .prepare(Command::Snapshot(Empty {}), Target::Queue)
                .is_err()
        );
        assert_eq!(requests.pending_for(&Target::Main).len(), 16);
        requests.invalidate(&Target::Main);
        for id in 1..=16 {
            assert!(requests.complete(event(Some(id), id)).is_none());
        }
        let rejected = requests
            .prepare(Command::Snapshot(Empty {}), Target::Main)
            .unwrap();
        requests.rejected(rejected.id);
        assert!(requests.is_idle());
        assert!(requests.complete(event(Some(rejected.id), 20)).is_none());
    }
}
