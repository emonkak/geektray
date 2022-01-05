use libdbus_sys as dbus;
use std::error;
use std::ffi::CStr;
use std::fmt;
use std::mem;
use std::os::raw::*;
use std::ptr;
use std::str;

#[derive(Debug)]
pub struct DBusConnection {
    connection: *mut dbus::DBusConnection,
}

impl DBusConnection {
    pub fn new(name: &CStr) -> Result<Self, DBusError> {
        let mut error = DBusError::init();

        let connection =
            unsafe { dbus::dbus_bus_get_private(dbus::DBusBusType::Session, error.as_mut_ptr()) };
        if error.is_set() {
            return Err(error);
        }

        unsafe {
            dbus::dbus_bus_request_name(
                connection,
                name.as_ptr() as *const c_char,
                dbus::DBUS_NAME_FLAG_REPLACE_EXISTING as c_uint,
                error.as_mut_ptr(),
            );
        }
        if error.is_set() {
            return Err(error);
        }

        Ok(Self { connection })
    }

    pub fn set_watch_functions(
        &self,
        add_function: dbus::DBusAddWatchFunction,
        remove_function: dbus::DBusRemoveWatchFunction,
        toggled_function: dbus::DBusWatchToggledFunction,
        data: *mut c_void,
        free_data_function: dbus::DBusFreeFunction,
    ) {
        unsafe {
            dbus::dbus_connection_set_watch_functions(
                self.connection,
                add_function,
                remove_function,
                toggled_function,
                data,
                free_data_function,
            );
        }
    }

    pub fn read_write(&self, timeout_milliseconds: c_int) -> bool {
        unsafe { dbus::dbus_connection_read_write(self.connection, timeout_milliseconds) != 0 }
    }

    pub fn pop_message(&self) -> Option<DBusMessage> {
        let message = unsafe { dbus::dbus_connection_pop_message(self.connection) };
        if !message.is_null() {
            Some(DBusMessage { message })
        } else {
            None
        }
    }

    pub fn send(&self, message: &DBusMessage, mut serial: Option<u32>) -> bool {
        unsafe {
            dbus::dbus_connection_send(
                self.connection,
                message.message,
                serial
                    .as_mut()
                    .map(|serial| serial as *mut _)
                    .unwrap_or(ptr::null_mut()),
            ) != 0
        }
    }

    pub fn flush(&self) {
        unsafe {
            dbus::dbus_connection_flush(self.connection);
        }
    }
}

impl Drop for DBusConnection {
    fn drop(&mut self) {
        unsafe {
            dbus::dbus_connection_close(self.connection);
        }
    }
}

pub struct DBusError {
    error: dbus::DBusError,
}

impl DBusError {
    pub fn init() -> Self {
        unsafe {
            let mut error = mem::MaybeUninit::uninit();
            dbus::dbus_error_init(error.as_mut_ptr());
            Self {
                error: error.assume_init(),
            }
        }
    }

    pub fn name(&self) -> Option<&str> {
        unsafe { c_str_to_slice(self.error.name) }
    }

    pub fn message(&self) -> Option<&str> {
        unsafe { c_str_to_slice(self.error.name) }
    }

    pub fn is_set(&self) -> bool {
        !self.error.name.is_null()
    }

    pub fn as_mut_ptr(&mut self) -> *mut dbus::DBusError {
        &mut self.error
    }
}

unsafe impl Send for DBusError {}

unsafe impl Sync for DBusError {}

impl error::Error for DBusError {}

impl fmt::Debug for DBusError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("DBusError")
            .field("name", &self.name())
            .field("message", &self.message())
            .finish()
    }
}

impl fmt::Display for DBusError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Some((name, message)) = self.message().zip(self.name()) {
            write!(f, "{} ({})", message, name)?;
        }
        Ok(())
    }
}

impl Drop for DBusError {
    fn drop(&mut self) {
        unsafe {
            dbus::dbus_error_free(&mut self.error);
        }
    }
}

pub struct DBusMessage {
    message: *mut dbus::DBusMessage,
}

impl DBusMessage {
    pub fn new_method_call(destination: &CStr, path: &CStr, iface: &CStr, method: &CStr) -> Self {
        let message = unsafe {
            dbus::dbus_message_new_method_call(
                destination.as_ptr() as *const i8,
                path.as_ptr() as *const i8,
                iface.as_ptr() as *const i8,
                method.as_ptr() as *const i8,
            )
        };
        Self { message }
    }

    pub fn new_method_return(&self) -> Self {
        let message = unsafe { dbus::dbus_message_new_method_return(self.message) };
        assert!(!message.is_null());
        Self { message }
    }

    pub fn add_arguments(&self, arguments: DBusArguments) {
        let mut args_iter = unsafe {
            let mut iter = mem::MaybeUninit::uninit();
            dbus::dbus_message_iter_init_append(self.message, iter.as_mut_ptr());
            iter.assume_init()
        };
        for argument in arguments.arguments {
            argument.append_to(&mut args_iter);
        }
    }

    pub fn message_type(&self) -> dbus::DBusMessageType {
        match unsafe { dbus::dbus_message_get_type(self.message) } {
            1 => dbus::DBusMessageType::MethodCall,
            2 => dbus::DBusMessageType::MethodReturn,
            3 => dbus::DBusMessageType::Error,
            4 => dbus::DBusMessageType::Signal,
            x => unreachable!("Invalid message type: {}", x),
        }
    }

    pub fn reply_serial(&self) -> u32 {
        unsafe { dbus::dbus_message_get_reply_serial(self.message) }
    }

    pub fn serial(&self) -> u32 {
        unsafe { dbus::dbus_message_get_serial(self.message) }
    }

    pub fn path(&self) -> Option<&str> {
        unsafe { c_str_to_slice(dbus::dbus_message_get_path(self.message)) }
    }

    pub fn interface(&self) -> Option<&str> {
        unsafe { c_str_to_slice(dbus::dbus_message_get_interface(self.message)) }
    }

    pub fn destination(&self) -> Option<&str> {
        unsafe { c_str_to_slice(dbus::dbus_message_get_destination(self.message)) }
    }

    pub fn member(&self) -> Option<&str> {
        unsafe { c_str_to_slice(dbus::dbus_message_get_member(self.message)) }
    }

    pub fn sender(&self) -> Option<&str> {
        unsafe { c_str_to_slice(dbus::dbus_message_get_sender(self.message)) }
    }

    pub fn no_reply(&self) -> bool {
        unsafe { dbus::dbus_message_get_no_reply(self.message) != 0 }
    }

    pub fn auto_start(&self) -> bool {
        unsafe { dbus::dbus_message_get_auto_start(self.message) != 0 }
    }
}

impl fmt::Debug for DBusMessage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("DBusMessage")
            .field("message_type", &self.message_type())
            .field("reply_serial", &self.reply_serial())
            .field("serial", &self.serial())
            .field("path", &self.path())
            .field("interface", &self.interface())
            .field("destination", &self.destination())
            .field("member", &self.member())
            .field("sender", &self.sender())
            .field("no_reply", &self.no_reply())
            .field("auto_start", &self.auto_start())
            .finish()
    }
}

impl Drop for DBusMessage {
    fn drop(&mut self) {
        unsafe {
            dbus::dbus_message_unref(self.message);
        }
    }
}

pub struct DBusArguments<'a> {
    arguments: Vec<Box<dyn 'a + DBusValue>>,
}

impl<'a> DBusArguments<'a> {
    pub fn new() -> Self {
        Self {
            arguments: Vec::new(),
        }
    }

    pub fn add_argument<T: 'a + DBusValue>(&mut self, argument: T) {
        self.arguments.push(Box::new(argument));
    }
}

pub trait DBusType {
    const TYPE: c_int;

    fn signature() -> String {
        (Self::TYPE as u8 as char).to_string()
    }
}

pub trait DBusValue {
    fn append_to(&self, message_iter: *mut dbus::DBusMessageIter);
}

macro_rules! impl_dbus_basic_type {
    ($type: ty, $signature: expr) => {
        impl DBusType for $type {
            const TYPE: c_int = $signature;
        }

        impl DBusValue for $type {
            fn append_to(&self, message_iter: *mut dbus::DBusMessageIter) {
                unsafe {
                    dbus::dbus_message_iter_append_basic(
                        message_iter,
                        $signature,
                        self as *const Self as *const c_void,
                    );
                }
            }
        }
    };
}

impl_dbus_basic_type!(bool, dbus::DBUS_TYPE_BOOLEAN);
impl_dbus_basic_type!(u8, dbus::DBUS_TYPE_BYTE);
impl_dbus_basic_type!(i16, dbus::DBUS_TYPE_INT16);
impl_dbus_basic_type!(u16, dbus::DBUS_TYPE_UINT16);
impl_dbus_basic_type!(i32, dbus::DBUS_TYPE_INT32);
impl_dbus_basic_type!(u32, dbus::DBUS_TYPE_UINT32);
impl_dbus_basic_type!(i64, dbus::DBUS_TYPE_INT64);
impl_dbus_basic_type!(u64, dbus::DBUS_TYPE_UINT64);
impl_dbus_basic_type!(f64, dbus::DBUS_TYPE_DOUBLE);

impl<'a> DBusType for &'a CStr {
    const TYPE: c_int = dbus::DBUS_TYPE_STRING;
}

impl<'a> DBusValue for &'a CStr {
    fn append_to(&self, message_iter: *mut dbus::DBusMessageIter) {
        unsafe {
            dbus::dbus_message_iter_append_basic(
                message_iter,
                dbus::DBUS_TYPE_STRING,
                &self.as_ptr() as *const _ as *const c_void,
            );
        }
    }
}

impl<T: DBusType> DBusType for Vec<T> {
    const TYPE: c_int = dbus::DBUS_TYPE_ARRAY;

    fn signature() -> String {
        format!("a{}", T::signature())
    }
}

impl<T: DBusType + DBusValue> DBusValue for Vec<T> {
    fn append_to(&self, message_iter: *mut dbus::DBusMessageIter) {
        unsafe {
            let mut array_iter = {
                let mut iter = mem::MaybeUninit::uninit();
                dbus::dbus_message_iter_open_container(
                    message_iter,
                    dbus::DBUS_TYPE_ARRAY,
                    T::signature().as_ptr() as *const i8,
                    iter.as_mut_ptr(),
                );
                iter.assume_init()
            };

            for element in self {
                element.append_to(&mut array_iter);
            }

            dbus::dbus_message_iter_close_container(message_iter, &mut array_iter);
        }
    }
}

impl<K: DBusType, V: DBusType> DBusType for (K, V) {
    const TYPE: c_int = dbus::DBUS_TYPE_DICT_ENTRY;

    fn signature() -> String {
        format!("{{{}{}}}", K::signature(), V::signature())
    }
}

impl<K: DBusValue, V: DBusType + DBusValue> DBusValue for (K, V) {
    fn append_to(&self, message_iter: *mut dbus::DBusMessageIter) {
        unsafe {
            let mut entry_iter = {
                let mut iter = mem::MaybeUninit::uninit();
                dbus::dbus_message_iter_open_container(
                    message_iter,
                    dbus::DBUS_TYPE_DICT_ENTRY,
                    ptr::null(),
                    iter.as_mut_ptr(),
                );
                iter.assume_init()
            };
            let mut value_iter = {
                let mut iter = mem::MaybeUninit::uninit();
                dbus::dbus_message_iter_open_container(
                    &mut entry_iter,
                    dbus::DBUS_TYPE_VARIANT,
                    V::signature().as_ptr() as *mut _,
                    iter.as_mut_ptr(),
                );
                iter.assume_init()
            };

            self.0.append_to(&mut entry_iter);
            self.1.append_to(&mut value_iter);

            dbus::dbus_message_iter_close_container(&mut value_iter, &mut entry_iter);
            dbus::dbus_message_iter_close_container(&mut entry_iter, &mut value_iter);
        }
    }
}

pub struct DBusVariant {
    value: Box<dyn DBusValue>,
    signature: String,
}

impl DBusVariant {
    pub fn new<T: 'static + DBusType + DBusValue>(value: T) -> Self {
        Self {
            value: Box::new(value),
            signature: T::signature(),
        }
    }
}

impl DBusType for DBusVariant {
    const TYPE: c_int = dbus::DBUS_TYPE_VARIANT;
}

impl DBusValue for DBusVariant {
    fn append_to(&self, message_iter: *mut dbus::DBusMessageIter) {
        unsafe {
            let mut variant_iter = {
                let mut iter = mem::MaybeUninit::uninit();
                dbus::dbus_message_iter_open_container(
                    message_iter,
                    dbus::DBUS_TYPE_VARIANT,
                    self.signature.as_ptr() as *const i8,
                    iter.as_mut_ptr(),
                );
                iter.assume_init()
            };

            self.value.append_to(&mut variant_iter);

            dbus::dbus_message_iter_close_container(message_iter, &mut variant_iter);
        }
    }
}

unsafe fn c_str_to_slice<'a>(c: *const c_char) -> Option<&'a str> {
    if c.is_null() {
        None
    } else {
        str::from_utf8(CStr::from_ptr(c).to_bytes()).ok()
    }
}
