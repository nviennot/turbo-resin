MEMORY
{
  /* The flash on the GD32F307 is 512K, in two different banks of 256KB.
   * The first bank has pages of 2KB, and the second bank 4KB.
   * There are delays when the CPU executes instructions from the second bank.
   */
  FLASH : ORIGIN = 0x08000000, LENGTH = 256K
  FLASH_RODATA : ORIGIN = ORIGIN(FLASH) + 256K, LENGTH = 256K
  RAM : ORIGIN = 0x20000000, LENGTH = 96K
}

SECTIONS {
  /*
   * We make different sections for lvgl, libs, and app.
   * The intent is to speed up flashing speed when developing as Jlink only
   * flashes what changes in the ROM. Changing the app code shouldn't change the
   * whole layout.
   */

  .lvgl.rodata ORIGIN(FLASH_RODATA) :
  {
    *liblvgl*:*(.rodata .rodata.*);
  } > FLASH_RODATA

  .libs.rodata ALIGN(4) :
  {
    *lib*:*(.rodata .rodata.*);
  } > FLASH_RODATA

  .app.rodata ALIGN(4) :
  {
    *(.rodata .rodata.*);
  } > FLASH_RODATA

  /* 0x150 is the end of the not yet defined vector_table */
  .lvgl.text ORIGIN(FLASH) + 0x150:
  {
    *liblvgl*:*(.text .text.*);
  } > FLASH

  .libs.text ALIGN(4) :
  {
    *lib*:*(.text .text.*);
  } > FLASH

_stext = ALIGN(4);

}
