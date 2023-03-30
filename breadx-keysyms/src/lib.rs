//               Copyright John Nunley, 2022.
// Distributed under the Boost Software License, Version 1.0.
//       (See accompanying file LICENSE or copy at
//         https://www.boost.org/LICENSE_1_0.txt)

//! A library for converting keycodes to key symbols.

#![allow(non_upper_case_globals)]
#![deprecated = "the `xkeysym` crate implements this crate without any cruft"]
#![forbid(unsafe_code, future_incompatible, rust_2018_idioms)]
#![no_std]

use breadx::{
    display::Cookie,
    prelude::*,
    protocol::xproto::{GetKeyboardMappingReply, Keycode, Keysym, Setup},
    Error, Result,
};
use keysyms::*;

const NO_SYMBOL: Keysym = 0;

#[path = "automatically_generated.rs"]
pub mod keysyms;

/// Keeps track of the keyboard state for the X11 display.
pub struct KeyboardState {
    innards: Innards,
}

enum Innards {
    /// The keyboard state hasn't been resolved.
    Unresolved(Cookie<GetKeyboardMappingReply>),
    /// The keyboard state has been resolved.
    Resolved(GetKeyboardMappingReply),
}

impl KeyboardState {
    /// Create a new `KeyboardState` associated with the given connection.
    pub fn new(dpy: &mut impl Display) -> Result<Self> {
        // open up the cookie
        let min_keycode = dpy.setup().min_keycode;
        let max_keycode = dpy.setup().max_keycode;
        let cookie = dpy.get_keyboard_mapping(min_keycode, max_keycode - min_keycode + 1)?;

        Ok(Self {
            innards: Innards::Unresolved(cookie),
        })
    }

    /// Create a new `KeyboardState`, async redox.
    #[cfg(feature = "async")]
    pub async fn new_async(dpy: &mut impl AsyncDisplay) -> Result<Self> {
        let min_keycode = dpy.setup().min_keycode;
        let max_keycode = dpy.setup().max_keycode;
        let cookie = dpy
            .get_keyboard_mapping(min_keycode, max_keycode - min_keycode + 1)
            .await?;

        Ok(Self {
            innards: Innards::Unresolved(cookie),
        })
    }

    /// Get the resolved keyboard mapping.
    fn resolve(&mut self, dpy: &mut impl Display) -> Result<&mut GetKeyboardMappingReply> {
        match self.innards {
            Innards::Unresolved(ref cookie) => {
                let reply = dpy.wait_for_reply(*cookie)?;
                self.innards = Innards::Resolved(reply);
                match &mut self.innards {
                    Innards::Resolved(reply) => Ok(reply),
                    _ => unreachable!(),
                }
            }
            Innards::Resolved(ref mut reply) => Ok(reply),
        }
    }

    #[cfg(feature = "async")]
    async fn resolve_async(
        &mut self,
        dpy: &mut impl AsyncDisplay,
    ) -> Result<&mut GetKeyboardMappingReply> {
        match self.innards {
            Innards::Unresolved(ref cookie) => {
                let reply = dpy.wait_for_reply(*cookie).await?;
                self.innards = Innards::Resolved(reply);
                match &mut self.innards {
                    Innards::Resolved(reply) => Ok(reply),
                    _ => unreachable!(),
                }
            }
            Innards::Resolved(ref mut reply) => Ok(reply),
        }
    }

    /// Refresh the keyboard mapping associated with this type.
    pub fn refresh(&mut self, dpy: &mut impl Display) -> Result<()> {
        let min_keycode = dpy.setup().min_keycode;
        let max_keycode = dpy.setup().max_keycode;

        // open up the cookie
        let cookie = dpy.get_keyboard_mapping(min_keycode, max_keycode - min_keycode + 1)?;

        self.innards = Innards::Unresolved(cookie);
        Ok(())
    }

    /// Refresh the keyboard mapping associated with this type, async redox.
    #[cfg(feature = "async")]
    pub async fn refresh_async(&mut self, dpy: &mut impl AsyncDisplay) -> Result<()> {
        let min_keycode = dpy.setup().min_keycode;
        let max_keycode = dpy.setup().max_keycode;

        // open up the cookie
        let cookie = dpy
            .get_keyboard_mapping(min_keycode, max_keycode - min_keycode + 1)
            .await?;

        self.innards = Innards::Unresolved(cookie);
        Ok(())
    }

    /// Get the keyboard symbol associated with the keycode and the
    /// column.
    pub fn symbol(
        &mut self,
        dpy: &mut impl Display,
        keycode: Keycode,
        column: u8,
    ) -> Result<Keysym> {
        let reply = self.resolve(dpy)?;
        get_symbol(dpy.setup(), reply, keycode, column)
    }

    /// Get the keyboard symbol associated with the keycode and the
    /// column, async redox.
    #[cfg(feature = "async")]
    pub async fn symbol_async(
        &mut self,
        dpy: &mut impl AsyncDisplay,
        keycode: Keycode,
        column: u8,
    ) -> Result<Keysym> {
        let reply = self.resolve_async(dpy).await?;
        get_symbol(dpy.setup(), reply, keycode, column)
    }
}

fn get_symbol(
    setup: &Setup,
    mapping: &GetKeyboardMappingReply,
    keycode: Keycode,
    mut column: u8,
) -> Result<Keysym> {
    // this is mostly a port of the logic from xcb keysyms
    let mut per = mapping.keysyms_per_keycode;
    if column >= per && column > 3 {
        return Err(Error::make_msg("Invalid column"));
    }

    // get the array of keysyms
    let start = (keycode - setup.min_keycode) as usize * per as usize;
    let end = start + per as usize;
    let keysyms = &mapping.keysyms[start..end];

    // get the alternate keysym if needed
    if column < 4 {
        if column > 1 {
            while per > 2 && keysyms[per as usize - 1] == NO_SYMBOL {
                per -= 1;
            }

            if per < 3 {
                column -= 2;
            }
        }

        if per <= column | 1 || keysyms[column as usize | 1] == NO_SYMBOL {
            // convert to upper/lower case
            let (upper, lower) = convert_case(keysyms[column as usize & !1]);
            if column & 1 == 0 {
                return Ok(lower);
            } else {
                return Ok(upper);
            }
        }
    }

    Ok(keysyms[column as usize])
}

/// Tell whether a keysym is a keypad key.
pub fn is_keypad_key(keysym: Keysym) -> bool {
    matches!(keysym, KEY_KP_Space..=KEY_KP_Equal)
}

/// Tell whether a keysym is a private keypad key.
pub fn is_private_keypad_key(keysym: Keysym) -> bool {
    matches!(keysym, 0x11000000..=0x1100FFFF)
}

/// Tell whether a keysym is a cursor key.
pub fn is_cursor_key(keysym: Keysym) -> bool {
    matches!(keysym, KEY_Home..=KEY_Select)
}

/// Tell whether a keysym is a PF key.
pub fn is_pf_key(keysym: Keysym) -> bool {
    matches!(keysym, KEY_KP_F1..=KEY_KP_F4)
}

/// Tell whether a keysym is a function key.
pub fn is_function_key(keysym: Keysym) -> bool {
    matches!(keysym, KEY_F1..=KEY_F35)
}

/// Tell whether a key is a miscellaneous function key.
pub fn is_misc_function_key(keysym: Keysym) -> bool {
    matches!(keysym, KEY_Select..=KEY_Break)
}

/// Tell whether a key is a modifier key.
pub fn is_modifier_key(keysym: Keysym) -> bool {
    matches!(
        keysym,
        KEY_Shift_L..=KEY_Hyper_R
         | KEY_ISO_Lock..=KEY_ISO_Level5_Lock
         | KEY_Mode_switch
         | KEY_Num_Lock
    )
}

/// Convert a keysym to its uppercase/lowercase equivalents.
fn convert_case(keysym: Keysym) -> (Keysym, Keysym) {
    // by default, they're both the regular keysym
    let (mut upper, mut lower) = (keysym, keysym);

    // tell which language it belongs to
    #[allow(non_upper_case_globals)]
    match keysym {
        KEY_A..=KEY_Z => lower += KEY_a - KEY_A,
        KEY_a..=KEY_z => upper -= KEY_a - KEY_A,
        KEY_Agrave..=KEY_Odiaeresis => lower += KEY_agrave - KEY_Agrave,
        KEY_agrave..=KEY_odiaeresis => upper -= KEY_agrave - KEY_Agrave,
        KEY_Ooblique..=KEY_Thorn => lower += KEY_oslash - KEY_Ooblique,
        KEY_oslash..=KEY_thorn => upper -= KEY_oslash - KEY_Ooblique,
        KEY_Aogonek => lower = KEY_aogonek,
        KEY_aogonek => upper = KEY_Aogonek,
        KEY_Lstroke..=KEY_Sacute => lower += KEY_lstroke - KEY_Lstroke,
        KEY_lstroke..=KEY_sacute => upper -= KEY_lstroke - KEY_Lstroke,
        KEY_Scaron..=KEY_Zacute => lower += KEY_scaron - KEY_Scaron,
        KEY_scaron..=KEY_zacute => upper -= KEY_scaron - KEY_Scaron,
        KEY_Zcaron..=KEY_Zabovedot => lower += KEY_zcaron - KEY_Zcaron,
        KEY_zcaron..=KEY_zabovedot => upper -= KEY_zcaron - KEY_Zcaron,
        KEY_Racute..=KEY_Tcedilla => lower += KEY_racute - KEY_Racute,
        KEY_racute..=KEY_tcedilla => upper -= KEY_racute - KEY_Racute,
        KEY_Hstroke..=KEY_Hcircumflex => lower += KEY_hstroke - KEY_Hstroke,
        KEY_hstroke..=KEY_hcircumflex => upper -= KEY_hstroke - KEY_Hstroke,
        KEY_Gbreve..=KEY_Jcircumflex => lower += KEY_gbreve - KEY_Gbreve,
        KEY_gbreve..=KEY_jcircumflex => upper -= KEY_gbreve - KEY_Gbreve,
        KEY_Cabovedot..=KEY_Scircumflex => lower += KEY_cabovedot - KEY_Cabovedot,
        KEY_cabovedot..=KEY_scircumflex => upper -= KEY_cabovedot - KEY_Cabovedot,
        KEY_Rcedilla..=KEY_Tslash => lower += KEY_rcedilla - KEY_Rcedilla,
        KEY_rcedilla..=KEY_tslash => upper -= KEY_rcedilla - KEY_Rcedilla,
        KEY_ENG => lower = KEY_eng,
        KEY_eng => upper = KEY_ENG,
        KEY_Amacron..=KEY_Umacron => lower += KEY_amacron - KEY_Amacron,
        KEY_amacron..=KEY_umacron => upper -= KEY_amacron - KEY_Amacron,
        KEY_Serbian_DJE..=KEY_Serbian_DZE => lower -= KEY_Serbian_DJE - KEY_Serbian_dje,
        KEY_Serbian_dje..=KEY_Serbian_dze => upper += KEY_Serbian_DJE - KEY_Serbian_dje,
        KEY_Cyrillic_YU..=KEY_Cyrillic_HARDSIGN => lower -= KEY_Cyrillic_YU - KEY_Cyrillic_yu,
        KEY_Cyrillic_yu..=KEY_Cyrillic_hardsign => upper += KEY_Cyrillic_YU - KEY_Cyrillic_yu,
        KEY_Greek_ALPHAaccent..=KEY_Greek_OMEGAaccent => {
            lower += KEY_Greek_alphaaccent - KEY_Greek_ALPHAaccent
        }
        KEY_Greek_alphaaccent..=KEY_Greek_omegaaccent
            if !matches!(
                keysym,
                KEY_Greek_iotaaccentdieresis | KEY_Greek_upsilonaccentdieresis
            ) =>
        {
            upper -= KEY_Greek_alphaaccent - KEY_Greek_ALPHAaccent
        }
        KEY_Greek_ALPHA..=KEY_Greek_OMEGA => lower += KEY_Greek_alpha - KEY_Greek_ALPHA,
        KEY_Greek_alpha..=KEY_Greek_omega if !matches!(keysym, KEY_Greek_finalsmallsigma) => {
            upper -= KEY_Greek_alpha - KEY_Greek_ALPHA
        }
        KEY_Armenian_AYB..=KEY_Armenian_fe => {
            lower |= 1;
            upper &= !1;
        }
        _ => {}
    }

    (upper, lower)
}
