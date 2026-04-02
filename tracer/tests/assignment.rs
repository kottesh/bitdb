/// Tests for work assignment - how files are distributed across threads.
use tracer::worker::{SlotState, assign_files};

#[test]
fn even_distribution_200_files_4_threads() {
    let file_ids: Vec<u32> = (1..=200).collect();
    let assignment = assign_files(&file_ids, 4);

    assert_eq!(assignment.len(), 4);
    for thread in &assignment {
        assert_eq!(thread.slots.len(), 50);
    }
}

#[test]
fn uneven_distribution_201_files_4_threads() {
    let file_ids: Vec<u32> = (1..=201).collect();
    let assignment = assign_files(&file_ids, 4);

    assert_eq!(assignment.len(), 4);
    assert_eq!(assignment[0].slots.len(), 51);
    assert_eq!(assignment[1].slots.len(), 50);
    assert_eq!(assignment[2].slots.len(), 50);
    assert_eq!(assignment[3].slots.len(), 50);
}

#[test]
fn single_file_4_threads_only_thread_0_gets_work() {
    let file_ids: Vec<u32> = vec![1];
    let assignment = assign_files(&file_ids, 4);

    assert_eq!(assignment.len(), 4);
    assert_eq!(assignment[0].slots.len(), 1);
    assert_eq!(assignment[1].slots.len(), 0);
    assert_eq!(assignment[2].slots.len(), 0);
    assert_eq!(assignment[3].slots.len(), 0);
}

#[test]
fn round_robin_order_is_correct() {
    // Thread 0 gets file ids at indices 0,4,8,...
    // Thread 1 gets file ids at indices 1,5,9,...
    let file_ids: Vec<u32> = (1..=12).collect();
    let assignment = assign_files(&file_ids, 4);

    assert_eq!(assignment[0].slots[0].file_id, 1);
    assert_eq!(assignment[0].slots[1].file_id, 5);
    assert_eq!(assignment[0].slots[2].file_id, 9);

    assert_eq!(assignment[1].slots[0].file_id, 2);
    assert_eq!(assignment[1].slots[1].file_id, 6);
    assert_eq!(assignment[1].slots[2].file_id, 10);

    assert_eq!(assignment[2].slots[0].file_id, 3);
    assert_eq!(assignment[3].slots[0].file_id, 4);
}

#[test]
fn serial_mode_all_files_go_to_thread_0() {
    let file_ids: Vec<u32> = (1..=20).collect();
    let assignment = assign_files(&file_ids, 1);

    assert_eq!(assignment.len(), 1);
    assert_eq!(assignment[0].slots.len(), 20);
    for (i, slot) in assignment[0].slots.iter().enumerate() {
        assert_eq!(slot.file_id, file_ids[i]);
    }
}

#[test]
fn all_slots_start_in_queued_state() {
    let file_ids: Vec<u32> = (1..=8).collect();
    let assignment = assign_files(&file_ids, 2);

    for thread in &assignment {
        for slot in &thread.slots {
            assert!(
                matches!(slot.state, SlotState::Queued),
                "slot for file {} should start Queued",
                slot.file_id
            );
        }
    }
}

#[test]
fn thread_ids_are_assigned_correctly() {
    let file_ids: Vec<u32> = (1..=4).collect();
    let assignment = assign_files(&file_ids, 4);

    for (i, thread) in assignment.iter().enumerate() {
        assert_eq!(thread.thread_id, i);
    }
}

#[test]
fn empty_file_list_produces_empty_slots_per_thread() {
    let assignment = assign_files(&[], 4);
    assert_eq!(assignment.len(), 4);
    for thread in &assignment {
        assert!(thread.slots.is_empty());
    }
}
