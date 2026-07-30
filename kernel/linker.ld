/* SPDX-License-Identifier: 0BSD */
ENTRY(_start)

SECTIONS
{
    /* Keep the fixed, identity-mapped kernel image clear of OVMF's dense
       low-memory boot allocations on a 256 MiB QEMU machine. */
    . = 0x04000000;
    __kernel_start = .;

    .text : ALIGN(0x1000)
    {
        KEEP(*(.text._start))
        *(.text .text.*)
    }

    .rodata : ALIGN(0x1000)
    {
        *(.rodata .rodata.*)
    }

    .data : ALIGN(0x1000)
    {
        *(.data .data.*)
    }

    .bss (NOLOAD) : ALIGN(0x1000)
    {
        __bss_start = .;
        *(.bss .bss.*)
        *(COMMON)
        __bss_end = .;
    }

    __kernel_end = ALIGN(0x1000);

    /DISCARD/ :
    {
        *(.eh_frame .eh_frame_hdr)
        *(.comment)
        *(.note .note.*)
    }
}
