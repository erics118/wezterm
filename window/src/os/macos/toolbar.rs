use super::{nsstring, nsstring_to_str};
use crate::color::SrgbaTuple;
use crate::{NativeTab, StatusIcon, ToolbarLabel, ToolbarMenuItem};
use cocoa::base::{id, nil};
use cocoa::foundation::{NSPoint, NSRect, NSSize};
use objc::declare::ClassDecl;
use objc::rc::StrongPtr;
use objc::runtime::{Class, Object, Sel, BOOL, NO, YES};
use objc::*;

const TOOLBAR: &str = "wezterm.toolbar";
const LABEL: &str = "wezterm.label";
const TABS: &str = "wezterm.tabs";
const GROUP_WIDTH: &str = "wezterm.tabs.width";
const CONTENT_WIDTH: &str = "wezterm.tabs.content-width";

// appkit values, some newer than the sdk this crate builds against
const TOOLBAR_STYLE_UNIFIED: isize = 3;
const TOOLBAR_DISPLAY_MODE_ICON_ONLY: usize = 2;
const WINDOW_TITLE_HIDDEN: isize = 1;
const ORIENTATION_HORIZONTAL: isize = 0;
const ORIENTATION_VERTICAL: isize = 1;
const LAYOUT_ATTRIBUTE_LEADING: isize = 5;
const LAYOUT_ATTRIBUTE_CENTER_Y: isize = 10;
const DISTRIBUTION_FILL_EQUALLY: isize = 1;
const LINE_BREAK_BY_CLIPPING: usize = 2;
const SCROLL_ELASTICITY_NONE: isize = 1;
const FONT_WEIGHT_REGULAR: f64 = 0.;
const FONT_WEIGHT_MEDIUM: f64 = 0.23;
const FONT_WEIGHT_SEMIBOLD: f64 = 0.3;
const PRIORITY_LOW: f32 = 1.;
const PRIORITY_REQUIRED: f32 = 1000.;
const IMAGE_SCALE_NONE: usize = 2;

// sizes in points, measured from activity monitor's toolbar segmented control
const GROUP_HEIGHT: f64 = 36.;
const GROUP_INSET: f64 = 4.;
const TAB_HEIGHT: f64 = GROUP_HEIGHT - 2. * GROUP_INSET;
// past this many points a tab stops shrinking, and the group scrolls instead
const TAB_MIN_WIDTH: f64 = 150.;
const TAB_MAX_WIDTH: f64 = 300.;
const GROUP_TRAILING: f64 = 40.;
const TITLE_SIZE: f64 = 14.;
const STATUS_LEADING: f64 = 10.;
const TITLE_TRAILING: f64 = 10.;
const TITLE_FADE: f64 = 24.;
const DIVIDER_HEIGHT: f64 = 11.;
const MENU_GAP: f64 = 4.;
const CLOSE_SIZE: f64 = 16.;
const STATUS_SIZE: f64 = 11.;
const STATUS_SLOT: f64 = 14.;
const CONTROL_STATE_MIXED: isize = -1;
const CONTROL_STATE_OFF: isize = 0;
const CONTROL_STATE_ON: isize = 1;
const STATUS_GAP: f64 = 6.;
const CLOSE_TRAILING: f64 = 8.;
// a press that moves less than this is a click, not a drag
const DRAG_THRESHOLD: f64 = 4.;
const SLIDE_DURATION: f64 = 0.15;
// the label keeps one width, so switching projects never moves the tabs
const LABEL_WIDTH: f64 = 190.;
const LABEL_PADDING: f64 = 12.;

// overlays of white, so the tabs follow the terminal background.
// the group fill stands in for the glass on systems without it
const GROUP_FILL: f64 = 0.12;
const ACTIVE_FILL: f64 = 0.10;
const DIVIDER: f64 = 0.16;

// NSTrackingArea options: entered and exited, in any app state, over the visible rect
const TRACKING_OPTIONS: usize = 0x01 | 0x80 | 0x200;

#[repr(C)]
struct EdgeInsets {
    top: f64,
    left: f64,
    bottom: f64,
    right: f64,
}

/// Adds or removes the toolbar to match the config
pub unsafe fn update(window: id, enabled: bool) {
    let toolbar: id = msg_send![window, toolbar];
    if !enabled {
        if !toolbar.is_null() {
            let _: () = msg_send![window, setToolbar: nil];
        }
        return;
    }
    if toolbar.is_null() {
        let toolbar: id = msg_send![class!(NSToolbar), alloc];
        let toolbar: id = msg_send![toolbar, initWithIdentifier: *nsstring(TOOLBAR)];
        let _: () = msg_send![toolbar, setDelegate: delegate()];
        let _: () = msg_send![toolbar, setAllowsUserCustomization: NO];
        let _: () = msg_send![toolbar, setDisplayMode: TOOLBAR_DISPLAY_MODE_ICON_ONLY];
        let _: () = msg_send![window, setToolbar: toolbar];
        let _: () = msg_send![toolbar, release];
        let _: () = msg_send![window, setToolbarStyle: TOOLBAR_STYLE_UNIFIED];
    }
    // the label item replaces the native title, which would push the tabs to the right
    let _: () = msg_send![window, setTitleVisibility: WINDOW_TITLE_HIDDEN];
}

/// Shows the label and gives it the menu it pops up, when the window has the toolbar
pub unsafe fn set_label(window: id, label: &ToolbarLabel) {
    let Some(view) = item_view(window, LABEL) else {
        return;
    };
    let subviews: id = msg_send![view, subviews];
    let row: id = msg_send![subviews, objectAtIndex: 0usize];
    fill_label(row, label);
    let _: () = msg_send![view, setMenu: menu(&label.menu)];
}

/// Opens the label's menu as a click on it does, when the window has the toolbar
pub unsafe fn show_label_menu(window: id) {
    if let Some(view) = item_view(window, LABEL) {
        let _: () = msg_send![view, mouseDown: nil];
    }
}

/// The colour, or the secondary label colour when None
unsafe fn color_or_secondary(color: Option<SrgbaTuple>) -> id {
    match color {
        Some(c) => msg_send![class!(NSColor),
            colorWithSRGBRed: c.0 as f64
            green: c.1 as f64
            blue: c.2 as f64
            alpha: c.3 as f64],
        None => msg_send![class!(NSColor), secondaryLabelColor],
    }
}

/// The icon's symbol in its colour, for a menu, which draws an image as it is
unsafe fn status_image(icon: &StatusIcon, point_size: f64) -> id {
    let image = symbol_image(&icon.symbol, point_size, FONT_WEIGHT_REGULAR);
    let colors: id = msg_send![class!(NSArray), arrayWithObject: color_or_secondary(icon.color)];
    let palette: id =
        msg_send![class!(NSImageSymbolConfiguration), configurationWithPaletteColors: colors];
    let image: id = msg_send![image, imageWithSymbolConfiguration: palette];
    // a template image takes its view's tint instead of the palette colour
    let _: () = msg_send![image, setTemplate: NO];
    image
}

/// A nil target sends each entry's action up the responder chain to the window view
unsafe fn menu(items: &[ToolbarMenuItem]) -> id {
    let menu: id = msg_send![class!(NSMenu), new];
    let _: () = msg_send![menu, autorelease];
    for item in items {
        let entry: id = if item.title.is_empty() {
            msg_send![class!(NSMenuItem), separatorItem]
        } else if item.header {
            msg_send![class!(NSMenuItem), sectionHeaderWithTitle: *nsstring(&item.title)]
        } else {
            let entry: id = msg_send![class!(NSMenuItem), alloc];
            let entry: id = msg_send![entry,
                initWithTitle: *nsstring(&item.title)
                action: sel!(weztermToolbarMenuItem:)
                keyEquivalent: *nsstring("")];
            let _: () = msg_send![entry, autorelease];
            let _: () = msg_send![entry, setRepresentedObject: *nsstring(&item.id)];
            let state = if item.checked {
                CONTROL_STATE_ON
            } else if item.mixed {
                CONTROL_STATE_MIXED
            } else {
                CONTROL_STATE_OFF
            };
            let _: () = msg_send![entry, setState: state];
            if let Some(icon) = &item.icon {
                let _: () = msg_send![entry, setImage: status_image(icon, STATUS_SIZE)];
            }
            entry
        };
        let _: () = msg_send![menu, addItem: entry];
    }
    menu
}

/// Replaces the tabs, when the window has the toolbar
pub unsafe fn set_tabs(window: id, tabs: &[NativeTab]) {
    let Some(container) = item_view(window, TABS) else {
        return;
    };
    let group = tab_stack(container);
    fill_tabs(group, tabs);
    fit(window);
    // the active tab scrolls into view when the tabs overflow the group
    if let Some(active) = tabs.iter().position(|tab| tab.active) {
        let _: () = msg_send![container, layoutSubtreeIfNeeded];
        let views: id = msg_send![group, arrangedSubviews];
        let view: id = msg_send![views, objectAtIndex: active];
        let bounds: NSRect = msg_send![view, bounds];
        let _: BOOL = msg_send![view, scrollRectToVisible: bounds];
    }
}

/// The stack of tabs inside the group's scroll view
unsafe fn tab_stack(container: id) -> id {
    let glass: BOOL = msg_send![container, respondsToSelector: sel!(contentView)];
    let scroll: id = if glass == YES {
        msg_send![container, contentView]
    } else {
        let subviews: id = msg_send![container, subviews];
        msg_send![subviews, objectAtIndex: 0usize]
    };
    msg_send![scroll, documentView]
}

/// Sets the width constraint with this identifier on view, adding it the first time
unsafe fn set_width(view: id, ident: &str, width: f64) {
    let constraints: id = msg_send![view, constraints];
    let count: usize = msg_send![constraints, count];
    let existing = (0..count).find_map(|i| {
        let constraint: id = msg_send![constraints, objectAtIndex: i];
        let constraint_ident: id = msg_send![constraint, identifier];
        (!constraint_ident.is_null() && nsstring_to_str(constraint_ident) == ident)
            .then_some(constraint)
    });
    match existing {
        Some(constraint) => {
            let _: () = msg_send![constraint, setConstant: width];
        }
        None => {
            let anchor: id = msg_send![view, widthAnchor];
            let constraint: id = msg_send![anchor, constraintEqualToConstant: width];
            let _: () = msg_send![constraint, setIdentifier: *nsstring(ident)];
            let _: () = msg_send![constraint, setActive: YES];
        }
    }
}

unsafe fn item_view(window: id, ident: &str) -> Option<id> {
    let toolbar: id = msg_send![window, toolbar];
    if toolbar.is_null() {
        return None;
    }
    let items: id = msg_send![toolbar, items];
    let count: usize = msg_send![items, count];
    (0..count).find_map(|i| {
        let item: id = msg_send![items, objectAtIndex: i];
        let item_ident: id = msg_send![item, itemIdentifier];
        if nsstring_to_str(item_ident) == ident {
            Some(msg_send![item, view])
        } else {
            None
        }
    })
}

unsafe fn white(alpha: f64) -> id {
    msg_send![class!(NSColor), colorWithWhite: 1.0f64 alpha: alpha]
}

/// A layer-backed view filled with a white overlay
unsafe fn fill(view: id, fill: f64, radius: f64) {
    let _: () = msg_send![view, setWantsLayer: YES];
    let layer: id = msg_send![view, layer];
    let fill: id = msg_send![white(fill), CGColor];
    let _: () = msg_send![layer, setBackgroundColor: fill];
    let _: () = msg_send![layer, setCornerRadius: radius];
}

unsafe fn constrain(anchor: id, relation: &str, constant: f64) {
    let constraint: id = match relation {
        ">=" => msg_send![anchor, constraintGreaterThanOrEqualToConstant: constant],
        _ => msg_send![anchor, constraintEqualToConstant: constant],
    };
    let _: () = msg_send![constraint, setActive: YES];
}

unsafe fn join(a: id, b: id, constant: f64) {
    let constraint: id = msg_send![a, constraintEqualToAnchor: b constant: constant];
    let _: () = msg_send![constraint, setActive: YES];
}

/// Pins every edge of view to the same edge of other
unsafe fn pin(view: id, other: id) {
    let _: () = msg_send![view, setTranslatesAutoresizingMaskIntoConstraints: NO];
    for (a, b) in [
        (
            msg_send![view, leadingAnchor],
            msg_send![other, leadingAnchor],
        ),
        (
            msg_send![view, trailingAnchor],
            msg_send![other, trailingAnchor],
        ),
        (msg_send![view, topAnchor], msg_send![other, topAnchor]),
        (
            msg_send![view, bottomAnchor],
            msg_send![other, bottomAnchor],
        ),
    ] as [(id, id); 4]
    {
        join(a, b, 0.);
    }
}

unsafe fn stack(class: &Class, orientation: isize, spacing: f64, insets: EdgeInsets) -> id {
    let stack: id = msg_send![class, new];
    let _: () = msg_send![stack, setOrientation: orientation];
    let _: () = msg_send![stack, setSpacing: spacing];
    let _: () = msg_send![stack, setEdgeInsets: insets];
    msg_send![stack, autorelease]
}

unsafe fn symbol_image(name: &str, point_size: f64, weight: f64) -> id {
    let image: id = msg_send![class!(NSImage),
        imageWithSystemSymbolName: *nsstring(name)
        accessibilityDescription: nil];
    let config: id = msg_send![class!(NSImageSymbolConfiguration),
        configurationWithPointSize: point_size
        weight: weight];
    msg_send![image, imageWithSymbolConfiguration: config]
}

/// The symbol at its own size, which a stack view may not stretch
unsafe fn symbol(name: &str, point_size: f64, weight: f64, color: id) -> id {
    let view: id = msg_send![class!(NSImageView),
        imageViewWithImage: symbol_image(name, point_size, weight)];
    let _: () = msg_send![view, setContentTintColor: color];
    let _: () = msg_send![view, setImageScaling: IMAGE_SCALE_NONE];
    for orientation in [ORIENTATION_HORIZONTAL, ORIENTATION_VERTICAL] {
        let _: () = msg_send![view, setContentHuggingPriority: PRIORITY_REQUIRED forOrientation: orientation];
        let _: () = msg_send![view, setContentCompressionResistancePriority: PRIORITY_REQUIRED forOrientation: orientation];
    }
    view
}

/// A one-line label that fades out its end instead of overflowing
unsafe fn fading_label(text: id) -> id {
    let field: id = msg_send![title_class(), labelWithAttributedString: text];
    let _: () = msg_send![field, setWantsLayer: YES];
    let layer: id = msg_send![field, layer];
    let mask: id = msg_send![class!(CAGradientLayer), layer];
    let _: () = msg_send![mask, setStartPoint: NSPoint::new(0., 0.5)];
    let _: () = msg_send![mask, setEndPoint: NSPoint::new(1., 0.5)];
    let _: () = msg_send![layer, setMask: mask];
    // it gives way first, so a long one fades instead of widening its container
    let _: () = msg_send![field, setContentCompressionResistancePriority: PRIORITY_LOW forOrientation: ORIENTATION_HORIZONTAL];
    field
}

unsafe fn styled(text: &str, font: id, color: id) -> StrongPtr {
    let paragraph = StrongPtr::new(msg_send![class!(NSMutableParagraphStyle), new]);
    let _: () = msg_send![*paragraph, setLineBreakMode: LINE_BREAK_BY_CLIPPING];
    let attrs: id = msg_send![class!(NSMutableDictionary), dictionary];
    let _: () = msg_send![attrs, setObject: font forKey: *nsstring("NSFont")];
    let _: () = msg_send![attrs, setObject: color forKey: *nsstring("NSColor")];
    let _: () = msg_send![attrs, setObject: *paragraph forKey: *nsstring("NSParagraphStyle")];
    let run: id = msg_send![class!(NSAttributedString), alloc];
    StrongPtr::new(msg_send![run, initWithString: *nsstring(text) attributes: attrs])
}

unsafe fn clear(stack: id) {
    let old: id = msg_send![stack, arrangedSubviews];
    let old: id = msg_send![old, copy];
    let count: usize = msg_send![old, count];
    for i in 0..count {
        let view: id = msg_send![old, objectAtIndex: i];
        let _: () = msg_send![stack, removeArrangedSubview: view];
        let _: () = msg_send![view, removeFromSuperview];
    }
    let _: () = msg_send![old, release];
}

/// The label's symbol, then its title over its subtitle, then a chevron when it has a menu
unsafe fn fill_label(row: id, label: &ToolbarLabel) {
    clear(row);
    let secondary: id = msg_send![class!(NSColor), secondaryLabelColor];
    let tertiary: id = msg_send![class!(NSColor), tertiaryLabelColor];
    let (title_color, subtitle_color): (id, id) = if label.dimmed {
        (secondary, tertiary)
    } else {
        (msg_send![class!(NSColor), labelColor], secondary)
    };
    if !label.symbol.is_empty() {
        let tint = color_or_secondary(label.symbol_color);
        let icon = symbol(&label.symbol, 14., FONT_WEIGHT_MEDIUM, tint);
        let _: () = msg_send![row, addArrangedSubview: icon];
    }

    let lines = stack(
        class!(NSStackView),
        ORIENTATION_VERTICAL,
        -2.,
        EdgeInsets {
            top: 0.,
            left: 0.,
            bottom: 0.,
            right: 0.,
        },
    );
    // text fields pad their lines, so the two overlap a little to fit the toolbar's height
    let _: () = msg_send![lines, setAlignment: LAYOUT_ATTRIBUTE_LEADING];
    let line_styles: [(&str, id, id); 2] = [
        (
            &label.title,
            msg_send![class!(NSFont), systemFontOfSize: 14.0f64 weight: FONT_WEIGHT_SEMIBOLD],
            title_color,
        ),
        (
            &label.subtitle,
            msg_send![class!(NSFont), systemFontOfSize: 11.0f64],
            subtitle_color,
        ),
    ];
    for (text, font, color) in line_styles {
        let _: () = msg_send![lines, addArrangedSubview: fading_label(*styled(text, font, color))];
    }
    // the text takes the label's spare width, which keeps the chevron at its end
    let _: () = msg_send![lines, setContentHuggingPriority: PRIORITY_LOW forOrientation: ORIENTATION_HORIZONTAL];
    let _: () = msg_send![lines, setContentCompressionResistancePriority: PRIORITY_LOW forOrientation: ORIENTATION_HORIZONTAL];
    let _: () = msg_send![row, addArrangedSubview: lines];
    if !label.menu.is_empty() {
        let chevron = symbol(
            "chevron.up.chevron.down",
            9.,
            FONT_WEIGHT_REGULAR,
            secondary,
        );
        let _: () = msg_send![row, addArrangedSubview: chevron];
    }
}

/// Equal-width tabs that share the group, so any count fits
unsafe fn fill_tabs(group: id, tabs: &[NativeTab]) {
    clear(group);
    for (idx, tab) in tabs.iter().enumerate() {
        let next_to_active = tab.active || (idx > 0 && tabs[idx - 1].active);
        let _: () =
            msg_send![group, addArrangedSubview: tab_view(idx, tab, idx > 0 && !next_to_active)];
    }
}

/// The title and a close button, with a pill behind the active tab and a divider before it.
/// every tab after the first keeps its divider and only hides it, so switching tabs moves nothing
unsafe fn tab_view(idx: usize, tab: &NativeTab, show_divider: bool) -> id {
    let view: id = msg_send![tab_class(), new];
    let _: () = msg_send![view, autorelease];
    (*view).set_ivar::<isize>("idx", idx as isize);
    (*view).set_ivar::<isize>("active", tab.active as isize);
    let _: () = msg_send![view, setTranslatesAutoresizingMaskIntoConstraints: NO];
    constrain(msg_send![view, heightAnchor], "=", TAB_HEIGHT);
    constrain(msg_send![view, widthAnchor], ">=", TAB_MIN_WIDTH);
    // every tab has a layer, since dragging slides them all
    let _: () = msg_send![view, setWantsLayer: YES];
    if tab.active {
        fill(view, ACTIVE_FILL, TAB_HEIGHT / 2.);
    }

    if idx > 0 {
        let divider: id = msg_send![class!(NSView), new];
        fill(divider, DIVIDER, 0.);
        let _: () = msg_send![divider, setAlphaValue: if show_divider { 1.0f64 } else { 0. }];
        let _: () = msg_send![view, addSubview: divider];
        let _: () = msg_send![divider, setTranslatesAutoresizingMaskIntoConstraints: NO];
        constrain(msg_send![divider, widthAnchor], "=", 1.);
        constrain(msg_send![divider, heightAnchor], "=", DIVIDER_HEIGHT);
        join(
            msg_send![divider, leadingAnchor],
            msg_send![view, leadingAnchor],
            0.,
        );
        join(
            msg_send![divider, centerYAnchor],
            msg_send![view, centerYAnchor],
            0.,
        );
        (*view).set_ivar::<id>("divider", divider);
        let _: () = msg_send![divider, release];
    }

    // a nil target sends the action up the responder chain to the window view.
    // it shows on hover, and otherwise only hides, so titles never move
    let close: id = msg_send![class!(NSButton),
        buttonWithImage: symbol_image("xmark", 9., FONT_WEIGHT_REGULAR)
        target: nil
        action: sel!(weztermCloseTab:)];
    let _: () = msg_send![close, setBordered: NO];
    let _: () = msg_send![close, setTag: idx as isize];
    let secondary: id = msg_send![class!(NSColor), secondaryLabelColor];
    let _: () = msg_send![close, setContentTintColor: secondary];
    let _: () = msg_send![close, setAlphaValue: 0.0f64];
    let _: () = msg_send![view, addSubview: close];
    let _: () = msg_send![close, setTranslatesAutoresizingMaskIntoConstraints: NO];
    constrain(msg_send![close, widthAnchor], "=", CLOSE_SIZE);
    constrain(msg_send![close, heightAnchor], "=", CLOSE_SIZE);
    join(
        msg_send![close, trailingAnchor],
        msg_send![view, trailingAnchor],
        -CLOSE_TRAILING,
    );
    join(
        msg_send![close, centerYAnchor],
        msg_send![view, centerYAnchor],
        0.,
    );
    (*view).set_ivar::<id>("close", close);

    // the status slot keeps its width when empty, so a status coming or going moves no title
    let slot: id = msg_send![class!(NSImageView), new];
    if let Some(status) = &tab.status {
        // an image view tints a symbol with its own colour, whatever the image's palette
        let _: () = msg_send![slot, setImage: symbol_image(&status.symbol, STATUS_SIZE, FONT_WEIGHT_REGULAR)];
        let _: () = msg_send![slot, setContentTintColor: color_or_secondary(status.color)];
    }
    let _: () = msg_send![view, addSubview: slot];
    let _: () = msg_send![slot, setTranslatesAutoresizingMaskIntoConstraints: NO];
    constrain(msg_send![slot, widthAnchor], "=", STATUS_SLOT);
    constrain(msg_send![slot, heightAnchor], "=", STATUS_SLOT);
    join(
        msg_send![slot, leadingAnchor],
        msg_send![view, leadingAnchor],
        STATUS_LEADING,
    );
    join(
        msg_send![slot, centerYAnchor],
        msg_send![view, centerYAnchor],
        0.,
    );
    let _: () = msg_send![slot, release];

    let title = fading_label(*attributed_title(tab));
    let _: () = msg_send![view, addSubview: title];
    let _: () = msg_send![title, setTranslatesAutoresizingMaskIntoConstraints: NO];
    join(
        msg_send![title, leadingAnchor],
        msg_send![slot, trailingAnchor],
        STATUS_GAP,
    );
    let title_trailing: id = msg_send![title, trailingAnchor];
    let close_leading: id = msg_send![close, leadingAnchor];
    let trailing: id = msg_send![title_trailing,
        constraintLessThanOrEqualToAnchor: close_leading
        constant: -TITLE_TRAILING];
    let _: () = msg_send![trailing, setActive: YES];
    join(
        msg_send![title, centerYAnchor],
        msg_send![view, centerYAnchor],
        0.,
    );
    view
}

/// The tabs of the group a tab is in, in order
unsafe fn siblings(tab: id) -> Vec<id> {
    let group: id = msg_send![tab, superview];
    let views: id = msg_send![group, arrangedSubviews];
    let count: usize = msg_send![views, count];
    (0..count)
        .map(|i| msg_send![views, objectAtIndex: i])
        .collect()
}

/// The slot a tab from idx lands in when shifted by dx, with every tab width wide
fn drop_index(idx: isize, dx: f64, width: f64, last: isize) -> isize {
    (idx + (dx / width).round() as isize).clamp(0, last)
}

/// Shifts a tab sideways, sliding it there when animated.
/// a view's backing layer ignores implicit animations, so the slide is an explicit one
unsafe fn slide(view: id, x: f64, animated: bool) {
    let layer: id = msg_send![view, layer];
    let key = nsstring("transform.translation.x");
    let to: id = msg_send![class!(NSNumber), numberWithDouble: x];
    let _: () = msg_send![class!(CATransaction), begin];
    let _: () = msg_send![class!(CATransaction), setDisableActions: YES];
    if animated {
        let presentation: id = msg_send![layer, presentationLayer];
        let from: id = if presentation.is_null() {
            msg_send![layer, valueForKeyPath: *key]
        } else {
            msg_send![presentation, valueForKeyPath: *key]
        };
        let animation: id = msg_send![class!(CABasicAnimation), animationWithKeyPath: *key];
        let _: () = msg_send![animation, setFromValue: from];
        let _: () = msg_send![animation, setToValue: to];
        let _: () = msg_send![animation, setDuration: SLIDE_DURATION];
        let timing: id =
            msg_send![class!(CAMediaTimingFunction), functionWithName: *nsstring("easeOut")];
        let _: () = msg_send![animation, setTimingFunction: timing];
        let _: () = msg_send![layer, addAnimation: animation forKey: *key];
    }
    let _: () = msg_send![layer, setValue: to forKeyPath: *key];
    let _: () = msg_send![class!(CATransaction), commit];
}

unsafe fn send_action(action: Sel, from: id) {
    let app: id = msg_send![class!(NSApplication), sharedApplication];
    let _: BOOL = msg_send![app, sendAction: action to: nil from: from];
}

/// Adds a tracking area over the visible rect, replacing any earlier one
extern "C" fn update_tracking_areas(this: &mut Object, _sel: Sel) {
    unsafe {
        let () = msg_send![super(this, class!(NSView)), updateTrackingAreas];
        let areas: id = msg_send![this, trackingAreas];
        let areas: id = msg_send![areas, copy];
        let count: usize = msg_send![areas, count];
        for i in 0..count {
            let area: id = msg_send![areas, objectAtIndex: i];
            let _: () = msg_send![this, removeTrackingArea: area];
        }
        let _: () = msg_send![areas, release];
        let area: id = msg_send![class!(NSTrackingArea), alloc];
        let zero = NSRect::new(NSPoint::new(0., 0.), NSSize::new(0., 0.));
        let area: id = msg_send![area,
            initWithRect: zero
            options: TRACKING_OPTIONS
            owner: this as *mut Object
            userInfo: nil];
        let _: () = msg_send![this, addTrackingArea: area];
        let _: () = msg_send![area, release];

        // a view made under the pointer, as a tab is when a click rebuilds the tabs, gets no entered event
        let window: id = msg_send![this, window];
        if window.is_null() {
            return;
        }
        let pointer: NSPoint = msg_send![window, mouseLocationOutsideOfEventStream];
        let local: NSPoint = msg_send![this, convertPoint: pointer fromView: nil];
        let bounds: NSRect = msg_send![this, bounds];
        if local.x >= 0.
            && local.x < bounds.size.width
            && local.y >= 0.
            && local.y < bounds.size.height
        {
            let _: () = msg_send![this, mouseEntered: nil];
        }
    }
}

extern "C" fn no(_this: &Object, _sel: Sel) -> BOOL {
    NO
}

extern "C" fn yes_for_event(_this: &Object, _sel: Sel, _event: id) -> BOOL {
    YES
}

/// A tab that a click activates and a drag moves, with its close button showing on hover
fn tab_class() -> &'static Class {
    extern "C" fn tag(this: &Object, _sel: Sel) -> isize {
        unsafe { *this.get_ivar::<isize>("idx") }
    }

    extern "C" fn target_index(this: &Object, _sel: Sel) -> isize {
        unsafe { *this.get_ivar::<isize>("target") }
    }

    // the close button takes its own clicks, and the tab every other one
    extern "C" fn hit_test(this: &Object, _sel: Sel, point: NSPoint) -> id {
        unsafe {
            let superview: id = msg_send![this, superview];
            let local: NSPoint = msg_send![this, convertPoint: point fromView: superview];
            let bounds: NSRect = msg_send![this, bounds];
            let inside = |rect: NSRect| {
                local.x >= rect.origin.x
                    && local.x < rect.origin.x + rect.size.width
                    && local.y >= rect.origin.y
                    && local.y < rect.origin.y + rect.size.height
            };
            if !inside(bounds) {
                return nil;
            }
            let close = *this.get_ivar::<id>("close");
            let alpha: f64 = msg_send![close, alphaValue];
            let frame: NSRect = msg_send![close, frame];
            if alpha > 0. && inside(frame) {
                close
            } else {
                this as *const Object as id
            }
        }
    }

    // the close button fades in and out
    extern "C" fn set_hover(this: &mut Object, hovered: bool) {
        unsafe {
            let close = *this.get_ivar::<id>("close");
            let _: () = msg_send![class!(NSAnimationContext), beginGrouping];
            let context: id = msg_send![class!(NSAnimationContext), currentContext];
            let _: () = msg_send![context, setDuration: SLIDE_DURATION];
            let animator: id = msg_send![close, animator];
            let _: () = msg_send![animator, setAlphaValue: if hovered { 1.0f64 } else { 0. }];
            let _: () = msg_send![class!(NSAnimationContext), endGrouping];
        }
    }

    extern "C" fn mouse_entered(this: &mut Object, _sel: Sel, _event: id) {
        set_hover(this, true);
    }

    extern "C" fn mouse_exited(this: &mut Object, _sel: Sel, _event: id) {
        set_hover(this, false);
    }

    extern "C" fn mouse_down(this: &mut Object, _sel: Sel, event: id) {
        unsafe {
            let point: NSPoint = msg_send![event, locationInWindow];
            this.set_ivar::<f64>("start_x", point.x);
            this.set_ivar::<isize>("dragging", 0);
        }
    }

    // the tab follows the pointer within the group, and every tab it passes slides the other way
    extern "C" fn mouse_dragged(this: &mut Object, _sel: Sel, event: id) {
        unsafe {
            let me = this as *mut Object;
            let point: NSPoint = msg_send![event, locationInWindow];
            let dx = point.x - *this.get_ivar::<f64>("start_x");
            let dragging = *this.get_ivar::<isize>("dragging") != 0;
            if !dragging && dx.abs() < DRAG_THRESHOLD {
                return;
            }
            let tabs = siblings(me);
            let idx = *this.get_ivar::<isize>("idx");
            let frame: NSRect = msg_send![me, frame];
            let width = frame.size.width;
            let last = tabs.len() as isize - 1;
            let dx = dx.clamp(-(idx as f64) * width, (last - idx) as f64 * width);
            if !dragging {
                this.set_ivar::<isize>("dragging", 1);
                let layer: id = msg_send![me, layer];
                let _: () = msg_send![layer, setZPosition: 1.0f64];
                // the dividers would sit between moving tabs, so they hide until the drop
                for &tab in &tabs {
                    let divider = *(*tab).get_ivar::<id>("divider");
                    if !divider.is_null() {
                        let _: () = msg_send![divider, setAlphaValue: 0.0f64];
                    }
                }
            }
            this.set_ivar::<f64>("drag_x", dx);
            slide(me, dx, false);
            let target = drop_index(idx, dx, width, last);
            for (i, &tab) in tabs.iter().enumerate() {
                let i = i as isize;
                if i == idx {
                    continue;
                }
                let offset = if idx < i && i <= target {
                    -width
                } else if target <= i && i < idx {
                    width
                } else {
                    0.
                };
                if *(*tab).get_ivar::<f64>("drag_x") != offset {
                    (*tab).set_ivar::<f64>("drag_x", offset);
                    slide(tab, offset, true);
                }
            }
        }
    }

    // a click activates the tab. a drag also slides it into its new slot and moves it there,
    // or slides it home when it is dropped where it started
    extern "C" fn mouse_up(this: &mut Object, _sel: Sel, _event: id) {
        unsafe {
            let me = this as *mut Object;
            send_action(sel!(weztermActivateTab:), me);
            if *this.get_ivar::<isize>("dragging") == 0 {
                return;
            }
            this.set_ivar::<isize>("dragging", 0);
            let tabs = siblings(me);
            let idx = *this.get_ivar::<isize>("idx");
            let frame: NSRect = msg_send![me, frame];
            let width = frame.size.width;
            let target = drop_index(
                idx,
                *this.get_ivar::<f64>("drag_x"),
                width,
                tabs.len() as isize - 1,
            );
            slide(me, (target - idx) as f64 * width, true);
            if target != idx {
                this.set_ivar::<isize>("target", target);
                send_action(sel!(weztermMoveTab:), me);
                return;
            }
            // nothing moved, so the tabs rebuild only if the activation changed, and the drag undoes itself
            let layer: id = msg_send![me, layer];
            let _: () = msg_send![layer, setZPosition: 0.0f64];
            let mut previous_active = true;
            for &tab in &tabs {
                let active = *(*tab).get_ivar::<isize>("active") != 0;
                let divider = *(*tab).get_ivar::<id>("divider");
                if !divider.is_null() {
                    let shown = !active && !previous_active;
                    let _: () = msg_send![divider, setAlphaValue: if shown { 1.0f64 } else { 0. }];
                }
                previous_active = active;
            }
        }
    }

    Class::get("WezTermTab").unwrap_or_else(|| {
        let mut cls =
            ClassDecl::new("WezTermTab", class!(NSView)).expect("Unable to register tab class");
        cls.add_ivar::<isize>("idx");
        cls.add_ivar::<isize>("target");
        cls.add_ivar::<isize>("active");
        cls.add_ivar::<f64>("start_x");
        // how far the tab is shifted: by the pointer when it is dragged, by the drag when it is passed
        cls.add_ivar::<f64>("drag_x");
        cls.add_ivar::<isize>("dragging");
        cls.add_ivar::<id>("close");
        cls.add_ivar::<id>("divider");
        unsafe {
            cls.add_method(sel!(tag), tag as extern "C" fn(&Object, Sel) -> isize);
            cls.add_method(
                sel!(targetIndex),
                target_index as extern "C" fn(&Object, Sel) -> isize,
            );
            cls.add_method(
                sel!(hitTest:),
                hit_test as extern "C" fn(&Object, Sel, NSPoint) -> id,
            );
            cls.add_method(
                sel!(mouseDownCanMoveWindow),
                no as extern "C" fn(&Object, Sel) -> BOOL,
            );
            cls.add_method(
                sel!(acceptsFirstMouse:),
                yes_for_event as extern "C" fn(&Object, Sel, id) -> BOOL,
            );
            cls.add_method(
                sel!(updateTrackingAreas),
                update_tracking_areas as extern "C" fn(&mut Object, Sel),
            );
            cls.add_method(
                sel!(mouseEntered:),
                mouse_entered as extern "C" fn(&mut Object, Sel, id),
            );
            cls.add_method(
                sel!(mouseExited:),
                mouse_exited as extern "C" fn(&mut Object, Sel, id),
            );
            cls.add_method(
                sel!(mouseDown:),
                mouse_down as extern "C" fn(&mut Object, Sel, id),
            );
            cls.add_method(
                sel!(mouseDragged:),
                mouse_dragged as extern "C" fn(&mut Object, Sel, id),
            );
            cls.add_method(
                sel!(mouseUp:),
                mouse_up as extern "C" fn(&mut Object, Sel, id),
            );
        }
        cls.register()
    })
}

/// The project label, which pops up its menu on a press
fn label_class() -> &'static Class {
    extern "C" fn hit_test(this: &Object, _sel: Sel, point: NSPoint) -> id {
        unsafe {
            let superview: id = msg_send![this, superview];
            let local: NSPoint = msg_send![this, convertPoint: point fromView: superview];
            let bounds: NSRect = msg_send![this, bounds];
            let inside = local.x >= 0.
                && local.x < bounds.size.width
                && local.y >= 0.
                && local.y < bounds.size.height;
            if inside {
                this as *const Object as id
            } else {
                nil
            }
        }
    }

    // the menu opens under the label, at least as wide
    extern "C" fn mouse_down(this: &mut Object, _sel: Sel, _event: id) {
        unsafe {
            let me = this as *mut Object;
            let menu: id = msg_send![me, menu];
            let count: isize = if menu.is_null() {
                0
            } else {
                msg_send![menu, numberOfItems]
            };
            if count == 0 {
                return;
            }
            let bounds: NSRect = msg_send![me, bounds];
            let _: () = msg_send![menu, setMinimumWidth: bounds.size.width];
            let flipped: BOOL = msg_send![me, isFlipped];
            let below = if flipped == YES {
                bounds.size.height + MENU_GAP
            } else {
                -MENU_GAP
            };
            let _: BOOL = msg_send![menu,
                popUpMenuPositioningItem: nil
                atLocation: NSPoint::new(0., below)
                inView: me];
        }
    }

    Class::get("WezTermToolbarLabel").unwrap_or_else(|| {
        let mut cls = ClassDecl::new("WezTermToolbarLabel", class!(NSView))
            .expect("Unable to register toolbar label class");
        unsafe {
            cls.add_method(
                sel!(hitTest:),
                hit_test as extern "C" fn(&Object, Sel, NSPoint) -> id,
            );
            cls.add_method(
                sel!(mouseDownCanMoveWindow),
                no as extern "C" fn(&Object, Sel) -> BOOL,
            );
            cls.add_method(
                sel!(acceptsFirstMouse:),
                yes_for_event as extern "C" fn(&Object, Sel, id) -> BOOL,
            );
            cls.add_method(
                sel!(mouseDown:),
                mouse_down as extern "C" fn(&mut Object, Sel, id),
            );
        }
        cls.register()
    })
}

unsafe fn attributed_title(tab: &NativeTab) -> StrongPtr {
    let title = StrongPtr::new(msg_send![class!(NSMutableAttributedString), new]);
    let font: id = msg_send![class!(NSFont), systemFontOfSize: TITLE_SIZE weight: 0.0f64];
    let paragraph = StrongPtr::new(msg_send![class!(NSMutableParagraphStyle), new]);
    let _: () = msg_send![*paragraph, setLineBreakMode: LINE_BREAK_BY_CLIPPING];

    for (text, color) in &tab.runs {
        let color: id = match color {
            Some(c) => msg_send![class!(NSColor),
                colorWithSRGBRed: c.0 as f64
                green: c.1 as f64
                blue: c.2 as f64
                alpha: c.3 as f64],
            None => msg_send![class!(NSColor), labelColor],
        };
        let attrs: id = msg_send![class!(NSMutableDictionary), dictionary];
        let _: () = msg_send![attrs, setObject: font forKey: *nsstring("NSFont")];
        let _: () = msg_send![attrs, setObject: color forKey: *nsstring("NSColor")];
        let _: () = msg_send![attrs, setObject: *paragraph forKey: *nsstring("NSParagraphStyle")];
        let run: id = msg_send![class!(NSAttributedString), alloc];
        let run = StrongPtr::new(msg_send![run, initWithString: *nsstring(text) attributes: attrs]);
        let _: () = msg_send![*title, appendAttributedString: *run];
    }
    title
}

/// Widens the tab group to end near the window's right edge, and its content to fit every tab.
/// a toolbar item's constraints may not reach outside it, so this reruns on every resize
pub unsafe fn fit(window: id) {
    if window.is_null() {
        return;
    }
    let Some(container) = item_view(window, TABS) else {
        return;
    };
    let min_width = TAB_MIN_WIDTH + 2. * GROUP_INSET;
    let in_window: id = msg_send![container, window];
    if in_window != window {
        // the toolbar drops a group left too wide by a shrinking window, so it shrinks to come back
        set_width(container, GROUP_WIDTH, min_width);
        let content: id = msg_send![window, contentView];
        let frame_view: id = msg_send![content, superview];
        let _: () = msg_send![frame_view, layoutSubtreeIfNeeded];
        let in_window: id = msg_send![container, window];
        if in_window != window {
            return;
        }
    }
    let bounds: NSRect = msg_send![container, bounds];
    let origin: NSRect = msg_send![container, convertRect: bounds toView: nil];
    let frame: NSRect = msg_send![window, frame];
    let group = tab_stack(container);
    let tabs: id = msg_send![group, arrangedSubviews];
    let count: usize = msg_send![tabs, count];
    // the space up to the window's edge, but no more than the tabs fill at their widest
    let width = (frame.size.width - origin.origin.x - GROUP_TRAILING)
        .min(count.max(1) as f64 * TAB_MAX_WIDTH + 2. * GROUP_INSET)
        .max(min_width);
    set_width(container, GROUP_WIDTH, width);
    set_width(
        group,
        CONTENT_WIDTH,
        width.max(count as f64 * TAB_MIN_WIDTH + 2. * GROUP_INSET),
    );
}

/// A label whose gradient mask fades its last points out, but only when the text overflows
fn title_class() -> &'static Class {
    extern "C" fn layout(this: &mut Object, _sel: Sel) {
        unsafe {
            let () = msg_send![super(this, class!(NSTextField)), layout];
            let layer: id = msg_send![this, layer];
            let mask: id = if layer.is_null() {
                nil
            } else {
                msg_send![layer, mask]
            };
            if mask.is_null() {
                return;
            }
            let bounds: NSRect = msg_send![this, bounds];
            let full: NSSize = msg_send![this, intrinsicContentSize];
            let width = bounds.size.width.max(1.);
            let fade_from = if full.width > width + 0.5 {
                ((width - TITLE_FADE) / width).max(0.)
            } else {
                1.
            };
            let colors: id = msg_send![class!(NSMutableArray), array];
            let opaque: id = msg_send![class!(NSColor), blackColor];
            let clear_color: id = msg_send![class!(NSColor), clearColor];
            for color in [opaque, opaque, clear_color] {
                let color: id = msg_send![color, CGColor];
                let _: () = msg_send![colors, addObject: color];
            }
            let locations: id = msg_send![class!(NSMutableArray), array];
            for location in [0., fade_from, 1.] {
                let number: id = msg_send![class!(NSNumber), numberWithDouble: location];
                let _: () = msg_send![locations, addObject: number];
            }
            let _: () = msg_send![class!(CATransaction), begin];
            let _: () = msg_send![class!(CATransaction), setDisableActions: YES];
            let _: () = msg_send![mask, setFrame: bounds];
            let _: () = msg_send![mask, setColors: colors];
            let _: () = msg_send![mask, setLocations: locations];
            let _: () = msg_send![class!(CATransaction), commit];
        }
    }

    Class::get("WezTermTabTitle").unwrap_or_else(|| {
        let mut cls = ClassDecl::new("WezTermTabTitle", class!(NSTextField))
            .expect("Unable to register tab title class");
        unsafe {
            cls.add_method(sel!(layout), layout as extern "C" fn(&mut Object, Sel));
        }
        cls.register()
    })
}

extern "C" fn item_identifiers(_this: &Object, _sel: Sel, _toolbar: id) -> id {
    unsafe {
        let idents: id = msg_send![class!(NSMutableArray), array];
        for ident in [LABEL, TABS] {
            let _: () = msg_send![idents, addObject: *nsstring(ident)];
        }
        idents
    }
}

extern "C" fn item_for_identifier(
    _this: &Object,
    _sel: Sel,
    _toolbar: id,
    ident: id,
    _will_insert: BOOL,
) -> id {
    unsafe {
        let item: id = msg_send![class!(NSToolbarItem), alloc];
        let item: id = msg_send![item, initWithItemIdentifier: ident];
        match nsstring_to_str(ident) {
            LABEL => {
                let row = stack(
                    class!(NSStackView),
                    ORIENTATION_HORIZONTAL,
                    8.,
                    EdgeInsets {
                        top: 0.,
                        left: LABEL_PADDING,
                        bottom: 0.,
                        right: LABEL_PADDING,
                    },
                );
                let _: () = msg_send![row, setAlignment: LAYOUT_ATTRIBUTE_CENTER_Y];
                let label: id = msg_send![label_class(), new];
                let _: () = msg_send![label, setTranslatesAutoresizingMaskIntoConstraints: NO];
                constrain(msg_send![label, widthAnchor], "=", LABEL_WIDTH);
                constrain(msg_send![label, heightAnchor], "=", GROUP_HEIGHT);
                let _: () = msg_send![label, addSubview: row];
                pin(row, label);
                let _: () = msg_send![item, setView: label];
                let _: () = msg_send![label, release];
            }
            TABS => {
                let group = stack(
                    class!(NSStackView),
                    ORIENTATION_HORIZONTAL,
                    0.,
                    EdgeInsets {
                        top: GROUP_INSET,
                        left: GROUP_INSET,
                        bottom: GROUP_INSET,
                        right: GROUP_INSET,
                    },
                );
                let _: () = msg_send![group, setDistribution: DISTRIBUTION_FILL_EQUALLY];
                let _: () = msg_send![group, setTranslatesAutoresizingMaskIntoConstraints: NO];

                // the tabs scroll sideways inside the rounded group once they reach their smallest width
                let scroll: id = msg_send![class!(NSScrollView), new];
                let _: () = msg_send![scroll, setDrawsBackground: NO];
                let _: () = msg_send![scroll, setHasHorizontalScroller: NO];
                let _: () = msg_send![scroll, setHasVerticalScroller: NO];
                let _: () = msg_send![scroll, setVerticalScrollElasticity: SCROLL_ELASTICITY_NONE];
                let _: () = msg_send![scroll, setDocumentView: group];
                let clip: id = msg_send![scroll, contentView];
                let _: () = msg_send![clip, setDrawsBackground: NO];
                join(
                    msg_send![group, leadingAnchor],
                    msg_send![clip, leadingAnchor],
                    0.,
                );
                join(msg_send![group, topAnchor], msg_send![clip, topAnchor], 0.);
                join(
                    msg_send![group, bottomAnchor],
                    msg_send![clip, bottomAnchor],
                    0.,
                );

                // the group sits on liquid glass where the system has it, like a native segmented control
                let container: id = match Class::get("NSGlassEffectView") {
                    Some(glass) => {
                        let container: id = msg_send![glass, new];
                        let _: () = msg_send![container, setCornerRadius: GROUP_HEIGHT / 2.];
                        let _: () = msg_send![container, setContentView: scroll];
                        container
                    }
                    None => {
                        let container: id = msg_send![class!(NSView), new];
                        fill(container, GROUP_FILL, GROUP_HEIGHT / 2.);
                        let layer: id = msg_send![container, layer];
                        let _: () = msg_send![layer, setMasksToBounds: YES];
                        let _: () = msg_send![container, addSubview: scroll];
                        pin(scroll, container);
                        container
                    }
                };
                let _: () = msg_send![container, setTranslatesAutoresizingMaskIntoConstraints: NO];
                constrain(msg_send![container, heightAnchor], "=", GROUP_HEIGHT);
                let _: () = msg_send![scroll, release];
                let _: () = msg_send![item, setView: container];
                let _: () = msg_send![container, release];
            }
            _ => {}
        }
        msg_send![item, autorelease]
    }
}

/// The toolbar keeps only a weak reference to its delegate, so one lives for the whole process
fn delegate() -> id {
    thread_local! {
        static DELEGATE: StrongPtr = unsafe {
            let cls = Class::get("WezTermToolbarDelegate").unwrap_or_else(|| {
                let mut cls = ClassDecl::new("WezTermToolbarDelegate", class!(NSObject))
                    .expect("Unable to register toolbar delegate class");
                cls.add_method(
                    sel!(toolbarDefaultItemIdentifiers:),
                    item_identifiers as extern "C" fn(&Object, Sel, id) -> id,
                );
                cls.add_method(
                    sel!(toolbarAllowedItemIdentifiers:),
                    item_identifiers as extern "C" fn(&Object, Sel, id) -> id,
                );
                cls.add_method(
                    sel!(toolbar:itemForItemIdentifier:willBeInsertedIntoToolbar:),
                    item_for_identifier as extern "C" fn(&Object, Sel, id, id, BOOL) -> id,
                );
                cls.register()
            });
            StrongPtr::new(msg_send![cls, new])
        };
    }
    DELEGATE.with(|delegate| **delegate)
}
