/* Linker script for GBA open-source BIOS */
/* Produces a flat 16384-byte binary at address 0x00000000 */

MEMORY
{
    BIOS (rx) : ORIGIN = 0x00000000, LENGTH = 16K
}

SECTIONS
{
    .text :
    {
        *(.text)
        *(.text.*)
        *(.rodata)
        *(.rodata.*)
        *(.data)
        *(.data.*)
    } > BIOS

    /* Pad output to exactly 16384 bytes */
    .pad (NOLOAD) :
    {
        . = ORIGIN(BIOS) + LENGTH(BIOS);
    } > BIOS

    /DISCARD/ :
    {
        *(.comment)
        *(.ARM.attributes)
        *(.ARM.exidx*)
        *(.note*)
    }
}
