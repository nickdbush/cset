use cset::{ChangeSet, Draft, Track, Trackable};

#[derive(Track, Debug, PartialEq)]
struct Point {
    x: i32,
    y: i32,
}

struct HistoryItem {
    point: usize,
    changeset: ChangeSet,
}

#[derive(Default)]
struct Document {
    points: Vec<Point>,
    undo_stack: Vec<HistoryItem>,
    redo_stack: Vec<HistoryItem>,
}

impl Document {
    fn undo(&mut self) {
        if let Some(history_item) = self.undo_stack.pop() {
            let point_id = history_item.point;
            let point = &mut self.points[point_id];

            let redo_changeset = point.apply_changeset(history_item.changeset);
            self.redo_stack.push(HistoryItem {
                point: point_id,
                changeset: redo_changeset,
            });
        }
    }

    fn redo(&mut self) {
        if let Some(history_item) = self.redo_stack.pop() {
            let point_id = history_item.point;
            let point = &mut self.points[point_id];

            let undo_changeset = point.apply_changeset(history_item.changeset);
            self.undo_stack.push(HistoryItem {
                point: point_id,
                changeset: undo_changeset,
            });
        }
    }

    fn set_point_pos(&mut self, id: usize, x: i32, y: i32) {
        self.redo_stack.clear();
        let undo_changeset = self.points[id].edit().set_x(x).set_y(y).commit();
        self.undo_stack.push(HistoryItem {
            point: id,
            changeset: undo_changeset,
        });
    }
}

fn main() {
    let mut doc = Document::default();
    doc.points.push(Point { x: 42, y: 21 });

    doc.set_point_pos(0, 10, 10);
    doc.set_point_pos(0, 20, 20);
    doc.set_point_pos(0, 30, 30);
    assert_eq!(doc.points[0], Point { x: 30, y: 30 });

    doc.undo();
    assert_eq!(doc.points[0], Point { x: 20, y: 20 });

    doc.undo();
    assert_eq!(doc.points[0], Point { x: 10, y: 10 });

    doc.redo();
    assert_eq!(doc.points[0], Point { x: 20, y: 20 });

    doc.set_point_pos(0, 40, 40);
    assert_eq!(doc.points[0], Point { x: 40, y: 40 });

    // As we just set a value, the redo stack should be empty
    doc.redo();
    assert_eq!(doc.points[0], Point { x: 40, y: 40 });
}
