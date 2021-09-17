//! Component for ProcessConsole, the command console.
//!
//! This provides one Component, ProcessConsoleComponent, which implements a
//! command console for controlling processes over a UART bus. On imix this is
//! typically USART3 (the DEBUG USB connector).
//!
//! Usage
//! -----
//! ```rust
//! let pconsole = ProcessConsoleComponent::new(board_kernel, uart_mux).finalize(());
//! ```

// Author: Philip Levis <pal@cs.stanford.edu>
// Last modified: 6/20/2018

use capsules::process_console;
use capsules::virtual_uart::{MuxUart, UartDevice};
use kernel::capabilities;
use kernel::component::Component;
use kernel::hil;
use kernel::process::ProcessPrinter;
use kernel::static_init;

pub struct ProcessConsoleComponent {
    board_kernel: &'static kernel::Kernel,
    uart_mux: &'static MuxUart<'static>,
    process_printer: &'static dyn ProcessPrinter,
}

impl ProcessConsoleComponent {
    pub fn new(
        board_kernel: &'static kernel::Kernel,
        uart_mux: &'static MuxUart,
        process_printer: &'static dyn ProcessPrinter,
    ) -> ProcessConsoleComponent {
        ProcessConsoleComponent {
            board_kernel: board_kernel,
            uart_mux: uart_mux,
            process_printer,
        }
    }
}

/// These constants are defined in the linker script for where the
/// kernel is placed in memory on chip.
extern "C" {
    static _estack: u8;
    static _sstack: u8;
    static _stext: u8;
    static _srodata: u8;
    static _etext: u8;
    static _srelocate: u8;
    static _erelocate: u8;
    static _szero: u8;
    static _ezero: u8;
}

pub struct Capability;
unsafe impl capabilities::ProcessManagementCapability for Capability {}

impl Component for ProcessConsoleComponent {
    type StaticInput = ();
    type Output = &'static process_console::ProcessConsole<'static, Capability>;

    unsafe fn finalize(self, _s: Self::StaticInput) -> Self::Output {
        // Create virtual device for console.
        let console_uart = static_init!(UartDevice, UartDevice::new(self.uart_mux, true));
        console_uart.setup();

        // Get addresses of where the kernel is placed to enable additional
        // debugging in process console.
        let kernel_addresses = process_console::KernelAddresses {
            stack_start: &_sstack as *const u8,
            stack_end: &_estack as *const u8,
            text_start: &_stext as *const u8,
            text_end: &_etext as *const u8,
            read_only_data_start: &_srodata as *const u8,
            relocations_start: &_srelocate as *const u8,
            relocations_end: &_erelocate as *const u8,
            bss_start: &_szero as *const u8,
            bss_end: &_ezero as *const u8,
        };

        let console = static_init!(
            process_console::ProcessConsole<'static, Capability>,
            process_console::ProcessConsole::new(
                console_uart,
                self.process_printer,
                &mut process_console::WRITE_BUF,
                &mut process_console::READ_BUF,
                &mut process_console::QUEUE_BUF,
                &mut process_console::COMMAND_BUF,
                self.board_kernel,
                kernel_addresses,
                Capability,
            )
        );
        hil::uart::Transmit::set_transmit_client(console_uart, console);
        hil::uart::Receive::set_receive_client(console_uart, console);

        console
    }
}
