ENTRY(_start)
SECTIONS {
  . = 0x08000000;
  .text : { *(.header) *(.text*) *(.rodata*) }
  /DISCARD/ : { *(.data*) *(.bss*) *(.comment) *(.ARM.*) }
}
