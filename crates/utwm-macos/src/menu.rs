//! The tray menu's pictures, drawn the way macOS draws the symbols in its own
//! menus: as template images, in the menu's text color, so light on a dark
//! menu, dark on a light one, and white on the highlighted item.
//!
//! Tauri's menus come from muda, which turns a picture into an ordinary image,
//! drawn in the colors it has, and has no way to ask for a template. Nor can
//! it ask macOS 27 to show the picture at all: from 27 on, AppKit hides menu
//! items' images unless an item asks for its image to be visible
//! (`preferredImageVisibility`, newer than the AppKit bindings this crate
//! uses). muda gives an item its picture before adding it to its menu, and
//! AppKit posts `NSMenuDidAddItemNotification` for every item added, so
//! settling both there catches each item long before the menu shows. The
//! app's pictures are all drawn to be templates, and its menus have no
//! others.

use std::ptr::NonNull;

use block2::RcBlock;
use objc2::runtime::NSObjectProtocol;
use objc2::{MainThreadMarker, msg_send, sel};
use objc2_app_kit::{NSMenu, NSMenuDidAddItemNotification, NSMenuItem};
use objc2_foundation::{NSNotification, NSNotificationCenter, NSNumber, ns_string};

/// `NSMenuItemImageVisibilityVisible`, from macOS 27's `NSMenuItem.h`.
const IMAGE_VISIBLE: isize = 1;

/// From now on, draws every picture added to a menu as a template, and shows
/// it on macOS 27 and later. Call it once, before the first menu is built.
pub fn draw_pictures_as_templates() {
    let settle = RcBlock::new(|notification: NonNull<NSNotification>| {
        // Menus belong to the main thread, which is where muda builds them.
        if MainThreadMarker::new().is_none() {
            return;
        }
        // SAFETY: the notification center passes a notification that lives
        // until the block returns.
        let notification = unsafe { notification.as_ref() };
        let Some(menu) = notification
            .object()
            .and_then(|object| object.downcast::<NSMenu>().ok())
        else {
            return;
        };
        let Some(index) = notification
            .userInfo()
            .and_then(|info| info.objectForKey(ns_string!("NSMenuItemIndex")))
            .and_then(|index| index.downcast::<NSNumber>().ok())
        else {
            return;
        };
        if let Some(item) = menu.itemAtIndex(index.integerValue()) {
            settle_picture(&item);
        }
    });
    // SAFETY: the name is AppKit's own constant. With no object to filter on
    // and no queue, the block runs on whichever thread posts, and it checks
    // that this is the main thread before it touches a menu.
    let observer = unsafe {
        NSNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(
            Some(NSMenuDidAddItemNotification),
            None,
            None,
            &settle,
        )
    };
    // The app builds its menu again whenever its status changes, for as long
    // as it runs, so the observer stays.
    std::mem::forget(observer);
}

fn settle_picture(item: &NSMenuItem) {
    let Some(image) = item.image() else {
        return;
    };
    image.setTemplate(true);
    if item.respondsToSelector(sel!(setPreferredImageVisibility:)) {
        // SAFETY: macOS 27 declares `preferredImageVisibility` an NSInteger
        // property of NSMenuItem, and this item answers its setter.
        let () = unsafe { msg_send![item, setPreferredImageVisibility: IMAGE_VISIBLE] };
    }
}
