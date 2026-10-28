use super::*;
use std::cmp::Reverse;

pub(super) const SLOTS: usize = 8;
const FAIR_WAIT: u64 = 8;

fn key(
    frame: u64,
    served: u64,
    priority: u8,
    dirty: bool,
    identity: u64,
) -> (bool, bool, u64, bool, Reverse<u8>, u64, u64) {
    let due = frame.saturating_sub(served) >= FAIR_WAIT;
    (
        priority < 5,
        !due,
        if due { served } else { 0 },
        !dirty,
        Reverse(priority),
        served,
        identity,
    )
}

pub(super) fn order(world: &World, entities: &mut [Entity], frame: u64) {
    entities.sort_by_key(|entity| {
        let preview = world.get::<EffectPreview>(*entity).unwrap();
        let dirty = preview.painted < 3
            || preview.last_quantity != preview.quantity
            || preview.last_numeric != preview.numeric
            || preview.observed.as_ref() != Some(&preview.effect)
            || preview.pending.is_some()
            || !preview.candidates.is_empty();
        let priority = world
            .get::<extent::Raster>(*entity)
            .map_or(if preview.priority { 4 } else { 1 }, |raster| {
                raster.priority
            });
        key(
            frame,
            preview.last_served,
            priority,
            dirty,
            entity.to_bits(),
        )
    });
}

pub(super) fn eligible(preview: &EffectPreview, changed: bool) -> bool {
    preview.working.is_some()
        || changed
        || !preview.candidates.is_empty()
        || preview
            .pending
            .is_some_and(|pending| pending.elapsed() >= Duration::from_millis(200))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focused_sources_keep_a_slot_while_background_editors_and_dirty_sources_remain_fair() {
        let mut served = [0; 64];
        let mut paints = [0; 64];
        for frame in 1..=32 {
            let mut order: Vec<_> = (0..64).collect();
            order.sort_by_key(|index| {
                key(
                    frame,
                    served[*index],
                    if *index == 0 {
                        5
                    } else if *index < 9 {
                        4
                    } else {
                        1
                    },
                    *index >= 9,
                    *index as u64,
                )
            });
            assert_eq!(order[0], 0);
            for index in order.into_iter().take(SLOTS) {
                served[index] = frame;
                paints[index] += 1;
            }
        }
        assert_eq!(paints[0], 32);
        assert!(paints.iter().all(|paints| *paints > 0), "{paints:?}");
    }

    #[test]
    fn more_focused_sources_than_slots_rotate_without_starving_the_oldest() {
        let mut served = [0; 64];
        for frame in 1..=8 {
            let mut order: Vec<_> = (0..64).collect();
            order.sort_by_key(|index| key(frame, served[*index], 5, true, *index as u64));
            for index in order.into_iter().take(SLOTS) {
                served[index] = frame;
            }
        }
        assert!(served.iter().all(|served| *served > 0));
    }

    #[test]
    fn changed_inputs_precede_idle_animations_between_fairness_deadlines() {
        let ordinary = key(10, 9, 1, true, 3);
        let editor = key(10, 9, 4, false, 1);
        let focus = key(10, 9, 5, false, 2);
        assert!(ordinary < editor);
        assert!(focus < ordinary);
        assert!(key(10, 1, 1, false, 4) < ordinary);
    }
}
