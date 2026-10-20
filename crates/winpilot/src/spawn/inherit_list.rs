use windows::Win32::{
    Foundation::HANDLE,
    System::Threading::{
        DeleteProcThreadAttributeList, InitializeProcThreadAttributeList, LPPROC_THREAD_ATTRIBUTE_LIST,
        PROC_THREAD_ATTRIBUTE_HANDLE_LIST, UpdateProcThreadAttribute,
    },
};
use winpilot_core::{Context, Error, Result};

const ATTRIBUTE_COUNT: u32 = 1;

pub struct InheritList {
    buffer: Vec<usize>,
    handles: Vec<HANDLE>,
}

impl InheritList {
    pub fn new(handles: Vec<HANDLE>) -> Result<Self> {
        let mut size = attribute_list_size()?;
        let mut buffer = vec![0usize; size.div_ceil(size_of::<usize>())];
        let list_ptr = LPPROC_THREAD_ATTRIBUTE_LIST(buffer.as_mut_ptr().cast());
        unsafe { InitializeProcThreadAttributeList(Some(list_ptr), ATTRIBUTE_COUNT, None, &raw mut size) }
            .context("preparing the worker handle list")?;
        let mut list = Self { buffer, handles };
        list.limit_to_handles()?;
        Ok(list)
    }

    fn limit_to_handles(&mut self) -> Result<()> {
        let value = self.handles.as_ptr().cast();
        let size = size_of_val(self.handles.as_slice());
        let attribute = PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize;
        unsafe { UpdateProcThreadAttribute(self.raw(), 0, attribute, Some(value), size, None, None) }
            .context("limiting the handles the worker inherits")
    }

    pub fn raw(&mut self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        LPPROC_THREAD_ATTRIBUTE_LIST(self.buffer.as_mut_ptr().cast())
    }
}

impl Drop for InheritList {
    fn drop(&mut self) {
        unsafe { DeleteProcThreadAttributeList(self.raw()) };
    }
}

fn attribute_list_size() -> Result<usize> {
    let mut size = 0;
    let probe = unsafe { InitializeProcThreadAttributeList(None, ATTRIBUTE_COUNT, None, &raw mut size) };
    if size > 0 {
        return Ok(size);
    }
    probe.context("sizing the worker handle list")?;
    Err(Error::internal("sizing the worker handle list returned no size"))
}
