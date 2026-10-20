use std::{ffi::c_void, mem::ManuallyDrop, ptr};

use windows::Win32::{
    Foundation::{VARIANT_FALSE, VARIANT_TRUE},
    System::{
        Com::SAFEARRAY,
        Ole::{SafeArrayAccessData, SafeArrayGetDim, SafeArrayGetLBound, SafeArrayGetUBound, SafeArrayUnaccessData},
        Variant::{VARENUM, VARIANT, VARIANT_0_0, VT_ARRAY, VT_BOOL, VT_BSTR, VT_I4, VariantClear},
    },
};
use windows::core::BSTR;

pub(crate) struct OwnedVariant(VARIANT);

impl OwnedVariant {
    pub(crate) fn from_raw(raw: VARIANT) -> Self {
        Self(raw)
    }

    pub(crate) fn flag(value: bool) -> Self {
        let mut variant = Self(VARIANT::default());
        let inner = variant.inner_mut();
        inner.vt = VT_BOOL;
        inner.Anonymous.boolVal = if value { VARIANT_TRUE } else { VARIANT_FALSE };
        variant
    }

    pub(crate) fn text(value: &str) -> Self {
        let mut variant = Self(VARIANT::default());
        let inner = variant.inner_mut();
        inner.vt = VT_BSTR;
        inner.Anonymous.bstrVal = ManuallyDrop::new(BSTR::from(value));
        variant
    }

    pub(crate) fn int(value: i32) -> Self {
        let mut variant = Self(VARIANT::default());
        let inner = variant.inner_mut();
        inner.vt = VT_I4;
        inner.Anonymous.lVal = value;
        variant
    }

    pub(crate) fn raw(&self) -> &VARIANT {
        &self.0
    }

    pub(crate) fn as_bool(&self) -> Option<bool> {
        (self.kind() == VT_BOOL).then(|| unsafe { self.inner().Anonymous.boolVal }.as_bool())
    }

    pub(crate) fn as_i32(&self) -> Option<i32> {
        (self.kind() == VT_I4).then(|| unsafe { self.inner().Anonymous.lVal })
    }

    pub(crate) fn as_string(&self) -> Option<String> {
        (self.kind() == VT_BSTR).then(|| unsafe { &self.inner().Anonymous.bstrVal }.to_string())
    }

    pub(crate) fn as_i32_array(&self) -> Option<Vec<i32>> {
        if self.kind() != (VT_ARRAY | VT_I4) {
            return None;
        }
        read_i32_array(unsafe { self.inner().Anonymous.parray })
    }

    fn kind(&self) -> VARENUM {
        self.inner().vt
    }

    fn inner(&self) -> &VARIANT_0_0 {
        unsafe { &self.0.Anonymous.Anonymous }
    }

    fn inner_mut(&mut self) -> &mut VARIANT_0_0 {
        unsafe { &mut self.0.Anonymous.Anonymous }
    }
}

impl Drop for OwnedVariant {
    fn drop(&mut self) {
        let _ = unsafe { VariantClear(&raw mut self.0) };
    }
}

fn read_i32_array(array: *const SAFEARRAY) -> Option<Vec<i32>> {
    if array.is_null() || unsafe { SafeArrayGetDim(array) } != 1 {
        return None;
    }
    let lower = unsafe { SafeArrayGetLBound(array, 1) }.ok()?;
    let upper = unsafe { SafeArrayGetUBound(array, 1) }.ok()?;
    let count = usize::try_from(upper.checked_sub(lower)?.checked_add(1)?).ok()?;
    let mut data: *mut c_void = ptr::null_mut();
    unsafe { SafeArrayAccessData(array, &raw mut data) }.ok()?;
    let values = copy_i32s(data, count);
    unsafe { SafeArrayUnaccessData(array) }.ok()?;
    Some(values)
}

fn copy_i32s(data: *const c_void, count: usize) -> Vec<i32> {
    if data.is_null() || count == 0 {
        return Vec::new();
    }
    unsafe { std::slice::from_raw_parts(data.cast::<i32>(), count) }.to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_round_trips() {
        let variant = OwnedVariant::text("Grüße");
        assert_eq!(variant.as_string().as_deref(), Some("Grüße"));
        assert_eq!(variant.as_i32(), None);
        assert_eq!(variant.as_bool(), None);
    }

    #[test]
    fn int_round_trips() {
        let variant = OwnedVariant::int(50004);
        assert_eq!(variant.as_i32(), Some(50004));
        assert_eq!(variant.as_string(), None);
        assert_eq!(variant.as_i32_array(), None);
    }

    #[test]
    fn flag_round_trips() {
        assert_eq!(OwnedVariant::flag(true).as_bool(), Some(true));
        assert_eq!(OwnedVariant::flag(false).as_bool(), Some(false));
    }

    #[test]
    fn empty_variant_reads_as_nothing() {
        let variant = OwnedVariant::from_raw(VARIANT::default());
        assert_eq!(variant.as_bool(), None);
        assert_eq!(variant.as_i32(), None);
        assert_eq!(variant.as_string(), None);
        assert_eq!(variant.as_i32_array(), None);
    }
}
