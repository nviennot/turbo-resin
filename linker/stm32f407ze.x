MEMORY
{
  FLASH : ORIGIN = 0x08000000, LENGTH = 512K
  RAM : ORIGIN = 0x20000000, LENGTH = 128K
}

SECTIONS {
  /*
   * We make different sections for lvgl, libs, and app.
   * The intent is to speed up flashing speed when developing as Jlink only
   * flashes what changes in the ROM. Changing the app code shouldn't change the
   * whole layout.
   */

   /* 0x188 is the end of the not yet defined vector_table */
   .lvgl.text ORIGIN(FLASH) + 0x188:
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

   _stext = ALIGN(4);

}
