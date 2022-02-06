target extended-remote :3333
#monitor arm semihosting enable
#monitor arm semihosting_fileio enable

define resume
  monitor resume
end

define reset
  monitor reset halt
end
