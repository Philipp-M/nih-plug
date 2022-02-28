use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};
use std::any::Any;
use std::pin::Pin;
use std::sync::Arc;

use crate::buffer::Buffer;
use crate::context::{GuiContext, ProcessContext};
use crate::param::internals::Params;

/// Basic functionality that needs to be implemented by a plugin. The wrappers will use this to
/// expose the plugin in a particular plugin format.
///
/// This is super basic, and lots of things I didn't need or want to use yet haven't been
/// implemented. Notable missing features include:
///
/// - Sidechain inputs
/// - Multiple output busses
/// - Special handling for offline processing
/// - Transport and other context information in the process call
/// - Sample accurate automation (this would be great, but sadly few hosts even support it so until
///   they do we'll ignore that it's a thing)
/// - Parameter hierarchies/groups
/// - Bypass parameters, right now the VST3 wrapper generates one for you
/// - Outputting parameter changes from the plugin
/// - MIDI CC handling
/// - Outputting MIDI events
#[allow(unused_variables)]
pub trait Plugin: Default + Send + Sync + 'static {
    const NAME: &'static str;
    const VENDOR: &'static str;
    const URL: &'static str;
    const EMAIL: &'static str;

    /// Semver compatible version string (e.g. `0.0.1`). Hosts likely won't do anything with this,
    /// but just in case they do this should only contain decimals values and dots.
    const VERSION: &'static str;

    /// The default number of inputs. Some hosts like, like Bitwig and Ardour, use the defaults
    /// instead of setting up the busses properly.
    const DEFAULT_NUM_INPUTS: u32 = 2;
    /// The default number of inputs. Some hosts like, like Bitwig and Ardour, use the defaults
    /// instead of setting up the busses properly.
    const DEFAULT_NUM_OUTPUTS: u32 = 2;

    /// Whether the plugin accepts note events. If this is set to `false`, then the plugin won't
    /// receive any note events.
    const ACCEPTS_MIDI: bool = false;

    /// The plugin's parameters. The host will update the parameter values before calling
    /// `process()`. These parameters are identified by strings that should never change when the
    /// plugin receives an update.
    fn params(&self) -> Pin<&dyn Params>;

    /// The plugin's editor, if it has one. The actual editor instance is created in
    /// [Editor::spawn]. A plugin editor likely wants to interact with the plugin's parameters and
    /// other shared data, so you'll need to move [Arc] pointing to any data you want to access into
    /// the editor. You can later modify the parameters through the [crate::GuiContext] and
    /// [crate::ParamSetter] after the editor GUI has been created.
    fn editor(&self) -> Option<Box<dyn Editor>> {
        None
    }

    //
    // The following functions follow the lifetime of the plugin.
    //

    /// Whether the plugin supports a bus config. This only acts as a check, and the plugin
    /// shouldn't do anything beyond returning true or false.
    fn accepts_bus_config(&self, config: &BusConfig) -> bool {
        config.num_input_channels == 2 && config.num_output_channels == 2
    }

    /// Initialize the plugin for the given bus and buffer configurations. If the plugin is being
    /// restored from an old state, then that state will have already been restored at this point.
    /// If based on those parameters (or for any reason whatsoever) the plugin needs to introduce
    /// latency, then you can do so here using the process context. Depending on how the host
    /// restores plugin state, this function may also be called twice in rapid succession. If the
    /// plugin fails to inialize for whatever reason, then this should return `false`.
    ///
    /// Before this point, the plugin should not have done any expensive initialization. Please
    /// don't be that plugin that takes twenty seconds to scan.
    fn initialize(
        &mut self,
        bus_config: &BusConfig,
        buffer_config: &BufferConfig,
        context: &mut impl ProcessContext,
    ) -> bool {
        true
    }

    /// Process audio. The host's input buffers have already been copied to the output buffers if
    /// they are not processing audio in place (most hosts do however). All channels are also
    /// guarenteed to contain the same number of samples. Lastly, denormals have already been taken
    /// case of by NIH-plug, and you can optionally enable the `assert_process_allocs` feature to
    /// abort the program when any allocation accurs in the process function while running in debug
    /// mode.
    ///
    /// TODO: Provide a way to access auxiliary input channels if the IO configuration is
    ///       assymetric
    /// TODO: Pass transport and other context information to the plugin
    fn process(&mut self, buffer: &mut Buffer, context: &mut impl ProcessContext) -> ProcessStatus;
}

/// Provides auxiliary metadata needed for a CLAP plugin.
pub trait ClapPlugin: Plugin {
    /// A unique ID that identifies this particular plugin. This is usually in reverse domain name
    /// notation, e.g. `com.manufacturer.plugin-name`.
    const CLAP_ID: &'static str;
    /// A short description for the plugin.
    const CLAP_DESCRIPTION: &'static str;
    /// Arbitrary keywords describing the plugin. See the CLAP specification for examples:
    /// <https://github.com/free-audio/clap/blob/main/include/clap/plugin.h>.
    //
    // TODO: CLAP mentions that `win32-dpi-aware` is a special keyword that informs the host that
    //       the plugin is DPI aware, can and should we have special handling for this?
    const CLAP_KEYWORDS: &'static [&'static str];
    /// A URL to the plugin's manual, CLAP does not specify what to do when there is none.
    //
    // TODO: CLAP does not specify this, can these manual fields be null pointers?
    const CLAP_MANUAL_URL: &'static str;
    /// A URL to the plugin's support page, CLAP does not specify what to do when there is none.
    const CLAP_SUPPORT_URL: &'static str;
}

/// Provides auxiliary metadata needed for a VST3 plugin.
pub trait Vst3Plugin: Plugin {
    /// The unique class ID that identifies this particular plugin. You can use the
    /// `*b"fooofooofooofooo"` syntax for this.
    ///
    /// This will be shuffled into a different byte order on Windows for project-compatibility.
    const VST3_CLASS_ID: [u8; 16];
    /// One or more categories, separated by pipe characters (`|`), up to 127 characters. Anything
    /// logner than that will be truncated. See the VST3 SDK for examples of common categories:
    /// <https://github.com/steinbergmedia/vst3_pluginterfaces/blob/2ad397ade5b51007860bedb3b01b8afd2c5f6fba/vst/ivstaudioprocessor.h#L49-L90>
    const VST3_CATEGORIES: &'static str;

    /// [Self::VST3_CLASS_ID] in the correct order for the current platform so projects and presets
    /// can be shared between platforms. This should not be overridden.
    const PLATFORM_VST3_CLASS_ID: [u8; 16] = swap_vst3_uid_byte_order(Self::VST3_CLASS_ID);
}

#[cfg(not(target_os = "windows"))]
const fn swap_vst3_uid_byte_order(uid: [u8; 16]) -> [u8; 16] {
    uid
}

#[cfg(target_os = "windows")]
const fn swap_vst3_uid_byte_order(mut uid: [u8; 16]) -> [u8; 16] {
    // No mutable references in const functions, so we can't use `uid.swap()`
    let original_uid = uid;

    uid[0] = original_uid[3];
    uid[1] = original_uid[2];
    uid[2] = original_uid[1];
    uid[3] = original_uid[0];

    uid[4] = original_uid[5];
    uid[5] = original_uid[4];
    uid[6] = original_uid[7];
    uid[7] = original_uid[6];

    uid
}

/// An editor for a [Plugin].
pub trait Editor: Send + Sync {
    /// Create an instance of the plugin's editor and embed it in the parent window. As explained in
    /// [Plugin::editor], you can then read the parameter values directly from your [crate::Params]
    /// object, and modifying the values can be done using the functions on the
    /// [crate::ParamSetter]. When you change a parameter value that way it will be broadcasted to
    /// the host and also updated in your [Params] struct.
    ///
    /// This function should return a handle to the editor, which will be dropped when the editor
    /// gets closed. Implement the [Drop] trait on the returned handle if you need to explicitly
    /// handle the editor's closing behavior.
    ///
    /// The wrapper guarantees that a previous handle has been dropped before this function is
    /// called again.
    //
    // TODO: Think of how this would work with the event loop. On Linux the wrapper must provide a
    //       timer using VST3's `IRunLoop` interface, but on Window and macOS the window would
    //       normally register its own timer. Right now we just ignore this because it would
    //       otherwise be basically impossible to have this still be GUI-framework agnostic. Any
    //       callback that deos involve actual GUI operations will still be spooled to the IRunLoop
    //       instance.
    fn spawn(&self, parent: ParentWindowHandle, context: Arc<dyn GuiContext>) -> Box<dyn Any>;

    /// Return the (currnent) size of the editor in pixels as a `(width, height)` pair.
    fn size(&self) -> (u32, u32);

    // TODO: Reconsider adding a tick function here for the Linux `IRunLoop`. To keep this platform
    //       and API agnostic, add a way to ask the GuiContext if the wrapper already provides a
    //       tick function. If it does not, then the Editor implementation must handle this by
    //       itself. This would also need an associated `PREFERRED_FRAME_RATE` constant.
    // TODO: Add the things needed for DPI scaling
    // TODO: Resizing
}

/// A raw window handle for platform and GUI framework agnostic editors.
pub struct ParentWindowHandle {
    pub handle: RawWindowHandle,
}

unsafe impl HasRawWindowHandle for ParentWindowHandle {
    fn raw_window_handle(&self) -> RawWindowHandle {
        self.handle
    }
}

/// We only support a single main input and output bus at the moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BusConfig {
    /// The number of input channels for the plugin.
    pub num_input_channels: u32,
    /// The number of output channels for the plugin.
    pub num_output_channels: u32,
}

/// Configuration for (the host's) audio buffers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BufferConfig {
    /// The current sample rate.
    pub sample_rate: f32,
    /// The maximum buffer size the host will use. The plugin should be able to accept variable
    /// sized buffers up to this size.
    pub max_buffer_size: u32,
}

/// Indicates the current situation after the plugin has processed audio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessStatus {
    /// Something went wrong while processing audio.
    Error(&'static str),
    /// The plugin has finished processing audio. When the input is silent, the most may suspend the
    /// plugin to save resources as it sees fit.
    Normal,
    /// The plugin has a (reverb) tail with a specific length in samples.
    Tail(u32),
    /// This plugin will continue to produce sound regardless of whether or not the input is silent,
    /// and should thus not be deactivated by the host. This is essentially the same as having an
    /// infite tail.
    KeepAlive,
}

/// Event for (incoming) notes. Right now this only supports a very small subset of the MIDI
/// specification. See the util module for convenient conversion functions.
///
/// All of the timings are sample offsets withing the current buffer.
///
/// TODO: Add more events as needed
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum NoteEvent {
    NoteOn {
        timing: u32,
        channel: u8,
        note: u8,
        velocity: u8,
    },
    NoteOff {
        timing: u32,
        channel: u8,
        note: u8,
        velocity: u8,
    },
}

impl NoteEvent {
    /// Return the sample within the current buffer this event belongs to.
    pub fn timing(&self) -> u32 {
        match &self {
            NoteEvent::NoteOn { timing, .. } => *timing,
            NoteEvent::NoteOff { timing, .. } => *timing,
        }
    }
}
