use std::{
    fmt::Formatter,
    marker::PhantomData,
};

mod private {
    pub trait Sealed {}
}

pub trait State: private::Sealed {
    type State = ();
    const VALUE: StateVal;
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Control<S: State> {
    discriminator: StateVal,
    state:         S::State,
    _marker:       PhantomData<S>,
}

macro_rules! decl_state {
    ($name:ident) => {
        decl_state!($name, ());
    };

    ($name:ident, $state:ty) => {
        #[derive(
            Debug, Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord, derive_more::Display,
        )]
        pub enum $name {}

        impl private::Sealed for $name {}

        impl State for $name {
            type State = $state;

            const VALUE: StateVal = StateVal::$name;
        }
    };
}

#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord, derive_more::Display)]
#[repr(u8)]
pub enum StateVal {
    FlightIdle,
    PingCentralStation,
    StartBLE,
    AwaitRoverStationary,
    CalibrateIMU,
    AntRun,
}

decl_state!(FlightIdle);
decl_state!(PingCentralStation);
decl_state!(StartBLE);
decl_state!(AwaitRoverStationary);
decl_state!(CalibrateIMU);
decl_state!(AntRun);

impl Control<FlightIdle> {
    #[inline]
    pub fn new() -> Self {
        Self {
            discriminator: StateVal::FlightIdle,
            state:         (),
            _marker:       PhantomData,
        }
    }
}

impl Default for Control<FlightIdle> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

macro_rules! decl_transition {
    ($name:ident, $from:ty => $to:ident) => {
        impl Control<$from> {
            #[inline]
            pub fn $name(self) -> Control<$to> {
                Control {
                    discriminator: StateVal::$to,
                    state:         (),
                    _marker:       PhantomData,
                }
            }
        }
    };
}

decl_transition!(voltage_supplied, FlightIdle => PingCentralStation);
decl_transition!(garage_pending, PingCentralStation => StartBLE);
decl_transition!(ble_started, StartBLE => AwaitRoverStationary);
decl_transition!(rover_stopped, AwaitRoverStationary => CalibrateIMU);
decl_transition!(calibration_complete, CalibrateIMU => AntRun);
decl_transition!(rover_moving, AntRun => AwaitRoverStationary);
