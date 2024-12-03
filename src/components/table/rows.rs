use super::cells::Cell;

#[derive(Default, Debug)]
pub struct Row<'a> {
    pub cells: Vec<Cell<'a>>,
    pub is_last: bool,
}

impl<'a, T> From<Vec<T>> for Row<'a>
where
    T: Into<Cell<'a>>,
{
    fn from(value: Vec<T>) -> Self {
        let len = value.len();
        let cells = value
            .into_iter()
            .enumerate()
            .map(|(index, item)| {
                let mut cell: Cell<'_> = item.into();
                cell.is_last = len == index + 1;
                cell
            })
            .collect();

        Self {
            cells,
            ..Default::default()
        }
    }
}
