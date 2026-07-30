/* SPDX-License-Identifier: 0BSD */
ENTRY(_start)

PHDRS
{
    image PT_LOAD FLAGS(5);
    stack PT_GNU_STACK FLAGS(6);
}

SECTIONS
{
    . = 0x40000000;

    .text : ALIGN(0x1000)
    {
        KEEP(*(.text._start))
        *(.text .text.*)
    } :image

    .rodata : ALIGN(16)
    {
        *(.rodata .rodata.*)
    } :image

    .data : ALIGN(16)
    {
        *(.data .data.*)
        *(.got .got.*)
    } :image

    .bss (NOLOAD) : ALIGN(16)
    {
        *(.bss .bss.*)
        *(COMMON)
    } :image

    /DISCARD/ :
    {
        *(.eh_frame .eh_frame_hdr)
        *(.comment)
        *(.note .note.*)
    }
}
