MEMORY
{
  /* The flash on the GD32F307 is 512K, in two different banks of 256KB.
   * The first bank has pages of 2KB, and the second bank 4KB.
   * There are delays when the CPU executes instructions from the second bank.
   * Let's ignore the second bank for now.
   */
  FLASH : ORIGIN = 0x08000000, LENGTH = 256K
  /*FLASH_RODATA : ORIGIN = 0x08000000, LENGTH = 256K*/
  RAM : ORIGIN = 0x20000000, LENGTH = 96K
}

/* This is where the call stack will be allocated. */
/* The stack is of the full descending type. */
/* You may want to use this variable to locate the call stack and static
   variables in different memory regions. Below is shown the default value */
/* _stack_start = ORIGIN(RAM) + LENGTH(RAM); */

/* You can use this symbol to customize the location of the .text section */
/* If omitted the .text section will be placed right after the .vector_table
   section */
/* This is required only on microcontrollers that store some configuration right
   after the vector table */
/* _stext = ORIGIN(FLASH) + 0x400; */

/* Example of putting non-initialized variables into custom RAM locations. */
/* This assumes you have defined a region RAM2 above, and in the Rust
   sources added the attribute `#[link_section = ".ram2bss"]` to the data
   you want to place there. */
/* Note that the section will not be zero-initialized by the runtime! */
/* SECTIONS {
     .ram2bss (NOLOAD) : ALIGN(4) {
       *(.ram2bss);
       . = ALIGN(4);
     } > RAM2
   } INSERT AFTER .bss;
*/

SECTIONS {
  /* 0x150 is the end of the not yet defined vector_table */
  .lvgl.text ORIGIN(FLASH) + 0x150:
  {
    *liblvgl*:*(.text .text.*);
  } > FLASH

  .lvgl.rodata ALIGN(4) :
  {
    *liblvgl*:*(.rodata .rodata.*);
  } > FLASH

  .libs.text ALIGN(4) :
  {
    *lib*:*(.text .text.*);
  } > FLASH

  .libs.rodata ALIGN(4) :
  {
    *lib*:*(.rodata .rodata.*);
  } > FLASH
}

/* Starts the main code section after the libraries */
_stext = ADDR(.libs.rodata) + SIZEOF(.libs.rodata);
