target extended-remote :2331
#monitor semihosting enable

define resume
  monitor go
end

define reset
  monitor reset
end
