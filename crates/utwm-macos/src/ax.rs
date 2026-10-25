//! An owned Accessibility element with the few attribute reads and writes the
//! window code needs.

use std::ffi::c_void;
use std::ptr::NonNull;

use objc2_application_services::{AXError, AXUIElement, AXValue, AXValueType};
use objc2_core_foundation::{CFArray, CFBoolean, CFRetained, CFString, CFType, CGPoint, CGSize};

/// An app, or one of its windows, as the Accessibility API sees it.
#[derive(Clone)]
pub(crate) struct Element(CFRetained<AXUIElement>);

// SAFETY: an AXUIElement is an immutable token for an object in another
// process. The Accessibility API may be called from any thread, and retaining
// and releasing a CF object is thread-safe.
unsafe impl Send for Element {}

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    /// The window server's number for a window element. Private, but stable
    /// for years and what Rectangle, yabai and AeroSpace all use to match
    /// Accessibility windows with the window server's list.
    fn _AXUIElementGetWindow(element: &AXUIElement, window: *mut u32) -> AXError;
}

impl Element {
    pub fn application(pid: i32) -> Self {
        // SAFETY: takes any process id; a process that is gone makes later
        // calls fail rather than misbehave.
        Self(unsafe { AXUIElement::new_application(pid) })
    }

    pub fn system_wide() -> Self {
        // SAFETY: no arguments.
        Self(unsafe { AXUIElement::new_system_wide() })
    }

    fn copy(&self, attribute: &str) -> Result<CFRetained<CFType>, AXError> {
        let name = CFString::from_str(attribute);
        let mut value: *const CFType = std::ptr::null();
        // SAFETY: `value` is a valid out-pointer for the duration of the call.
        let error = unsafe {
            self.0
                .copy_attribute_value(&name, NonNull::from(&mut value))
        };
        if error != AXError::Success {
            return Err(error);
        }
        let value = NonNull::new(value.cast_mut()).ok_or(AXError::NoValue)?;
        // SAFETY: a successful copy hands back a +1 reference we now own.
        Ok(unsafe { CFRetained::from_raw(value) })
    }

    /// An attribute holding another element, such as `AXFocusedWindow`.
    pub fn element(&self, attribute: &str) -> Option<Element> {
        self.copy(attribute)
            .ok()?
            .downcast::<AXUIElement>()
            .ok()
            .map(Element)
    }

    /// An attribute holding a list of elements, such as `AXWindows`.
    pub fn elements(&self, attribute: &str) -> Vec<Element> {
        let Some(array) = self
            .copy(attribute)
            .ok()
            .and_then(|value| value.downcast::<CFArray>().ok())
        else {
            return Vec::new();
        };
        // SAFETY: every element of a CFArray is a CF object.
        let array = unsafe { array.cast_unchecked::<CFType>() };
        array
            .iter()
            .filter_map(|item| item.downcast::<AXUIElement>().ok().map(Element))
            .collect()
    }

    pub fn string(&self, attribute: &str) -> Option<String> {
        let value = self.copy(attribute).ok()?.downcast::<CFString>().ok()?;
        Some(value.to_string())
    }

    pub fn bool(&self, attribute: &str) -> Option<bool> {
        let value = self.copy(attribute).ok()?.downcast::<CFBoolean>().ok()?;
        Some(value.as_bool())
    }

    pub fn point(&self, attribute: &str) -> Option<CGPoint> {
        let mut point = CGPoint::default();
        self.value(attribute, AXValueType::CGPoint, &mut point)
            .then_some(point)
    }

    pub fn size(&self, attribute: &str) -> Option<CGSize> {
        let mut size = CGSize::default();
        self.value(attribute, AXValueType::CGSize, &mut size)
            .then_some(size)
    }

    /// Reads an AXValue attribute of `kind` into `out`, which must be the
    /// matching Core Graphics type.
    fn value<T>(&self, attribute: &str, kind: AXValueType, out: &mut T) -> bool {
        let Some(value) = self
            .copy(attribute)
            .ok()
            .and_then(|value| value.downcast::<AXValue>().ok())
        else {
            return false;
        };
        // SAFETY: `kind` names the type `out` points at (CGPoint or CGSize),
        // and AXValueGetValue checks the stored type before writing.
        unsafe { value.value(kind, NonNull::from(out).cast::<c_void>()) }
    }

    pub fn set_point(&self, attribute: &str, point: CGPoint) -> Result<(), AXError> {
        let mut point = point;
        // SAFETY: CGPoint is the type AXValueType::CGPoint describes; the value
        // is copied before the call returns.
        let value = unsafe {
            AXValue::new(
                AXValueType::CGPoint,
                NonNull::from(&mut point).cast::<c_void>(),
            )
        }
        .ok_or(AXError::Failure)?;
        self.set(attribute, &value)
    }

    pub fn set_size(&self, attribute: &str, size: CGSize) -> Result<(), AXError> {
        let mut size = size;
        // SAFETY: CGSize is the type AXValueType::CGSize describes; the value
        // is copied before the call returns.
        let value = unsafe {
            AXValue::new(
                AXValueType::CGSize,
                NonNull::from(&mut size).cast::<c_void>(),
            )
        }
        .ok_or(AXError::Failure)?;
        self.set(attribute, &value)
    }

    pub fn set_bool(&self, attribute: &str, value: bool) -> Result<(), AXError> {
        self.set(attribute, CFBoolean::new(value))
    }

    fn set(&self, attribute: &str, value: &CFType) -> Result<(), AXError> {
        let name = CFString::from_str(attribute);
        // SAFETY: both arguments are live CF objects for the whole call.
        let error = unsafe { self.0.set_attribute_value(&name, value) };
        if error == AXError::Success {
            Ok(())
        } else {
            Err(error)
        }
    }

    pub fn settable(&self, attribute: &str) -> bool {
        let name = CFString::from_str(attribute);
        let mut settable: u8 = 0;
        // SAFETY: `settable` is a valid out-pointer for a Boolean.
        let error = unsafe {
            self.0
                .is_attribute_settable(&name, NonNull::from(&mut settable))
        };
        error == AXError::Success && settable != 0
    }

    pub fn perform(&self, action: &str) -> Result<(), AXError> {
        let name = CFString::from_str(action);
        // SAFETY: `name` is a live CFString for the whole call.
        let error = unsafe { self.0.perform_action(&name) };
        if error == AXError::Success {
            Ok(())
        } else {
            Err(error)
        }
    }

    /// The window server's number for this window, if it is one.
    pub fn window_id(&self) -> Option<u32> {
        let mut id = 0_u32;
        // SAFETY: `id` is a valid out-pointer; the element is live.
        let error = unsafe { _AXUIElementGetWindow(&self.0, &mut id) };
        (error == AXError::Success && id != 0).then_some(id)
    }

    /// How long to wait for an app before giving up on it. On the system-wide
    /// element this sets the default for every element.
    pub fn set_timeout(&self, seconds: f32) {
        // SAFETY: plain call on a live element.
        let _ = unsafe { self.0.set_messaging_timeout(seconds) };
    }
}
