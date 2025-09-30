use ratatui::layout::{Direction, Flex};
use ratatui_kit::{AnyElement, Props, component, element, prelude::View, with_layout_style};

#[with_layout_style(gap, margin, offset, width, height, justify_content)]
#[derive(Default, Props)]
pub struct FlexProps<'a> {
    pub children: Vec<AnyElement<'a>>,
    pub vertical: bool,
    pub align_items: Flex,
}

#[component]
pub fn FlexView<'a>(props: &mut FlexProps<'a>) -> impl Into<AnyElement<'a>> {
    let vertical = props.vertical;
    element!(
        View(
            flex_direction: if vertical {
                Direction::Vertical
            }else{
                Direction::Horizontal
            },
            justify_content: props.justify_content,
            gap: props.gap,
            margin: props.margin,
            offset: props.offset,
            width: props.width,
            height: props.height,
        ){
            #(props.children.iter_mut().map(|child|{
                element!(
                    View(
                        flex_direction: {
                            if !vertical {
                                Direction::Vertical
                            }else{
                                Direction::Horizontal
                            }
                        },
                        justify_content: props.align_items,
                    ){
                        #(child)
                    }
                )
            }))
        }
    )
}
