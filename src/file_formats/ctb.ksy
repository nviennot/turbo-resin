# SPDX-License-Identifier: GPL-3.0-or-later

# Based on https://github.com/sn4k3/UVtools/blob/master/UVtools.Core/FileFormats/ChituboxFile.cs

# Missing:
# PrintParametersV4
# LayerExtended

meta:
  id: ctb
  file-extension: ctb
  endian: le
seq:
  - id: header
    type: header
instances:
  print_settings:
    type: print_settings
    pos: header.print_settings_offset
  slicer_settings:
    type: slicer_settings
    pos: header.slicer_settings_offset
  layers:
    type: layer
    pos: header.layers_offset
    repeat: expr
    repeat-expr: header.num_layers
  small_preview:
    type: preview
    pos: header.small_preview_offset
  large_preview:
    type: preview
    pos: header.large_preview_offset
types:
  header:
    seq:
      - id: magic
        # 0x12FD0086 for CTB, 0x12FD0106 for CTBv4
        contents: [0x86, 0x00, 0xfd, 0x12]
      - id: version
        type: u4
      - id: bed_size_x
        type: f4
      - id: bed_size_y
        type: f4
      - id: bed_size_z
        type: f4
      - id: unknown1
        type: u4
      - id: unknown2
        type: u4
      - id: height_mm
        type: f4
      # The following fields are informational.
      # The effective values are in the per layer struct
      - id: layer_height_mm
        type: f4
      - id: normal_exposure_duration_sec
        type: f4
      - id: bottom_exposure_duration_sec
        type: f4
      - id: light_off_delay_duration_sec
        type: f4
      - id: num_bottom_layers
        type: u4
      - id: resolution_x
        type: u4
      - id: resolution_y
        type: u4
      - id: large_preview_offset
        type: u4
      - id: layers_offset
        type: u4
      - id: num_layers
        type: u4
      - id: small_preview_offset
        type: u4
      - id: print_duration_sec
        type: u4
      - id: image_mirrored
        type: u4
      - id: print_settings_offset
        type: u4
      - id: print_settings_size
        type: u4
      - id: anti_aliasing_level
        type: u4
      - id: normal_uv_power # 0x00 to 0xFF
        type: u2
      - id: bottom_uv_power # 0x00 to 0xFF
        type: u2
      - id: encryption_key
        type: u4
      - id: slicer_settings_offset
        type: u4
      - id: slicer_settings_size
        type: u4
  print_settings:
    seq:
      - id: bottom_lift_height_mm
        type: f4
      - id: bottom_lift_speed_mm_per_min
        type: f4
      - id: normal_lift_height_mm
        type: f4
      - id: normal_lift_speed_mm_per_min
        type: f4
      - id: normal_retract_speed_mm_per_min
        type: f4
      - id: volume_ml
        type: f4
      - id: weight_g
        type: f4
      - id: cost_dollars
        type: f4
      - id: bottom_light_off_delay_sec
        type: f4
      - id: normal_light_off_delay_sec
        type: f4
      - id:  bottom_layer_count
        type: u4
      - id: unknown_1
        type: u4
      - id: unknown_2
        type: u4
      - id: unknown_3
        type: u4
      - id: unknown_4
        type: u4
  slicer_settings:
    seq:
      - id: bottom_lift_height2_mm
        type: f4
      - id: bottom_lift_speed2_mm_sec
        type: f4
      - id: normal_lift_height2_mm
        type: f4
      - id: normal_lift_speed2_mm_sec
        type: f4
      - id: retract_height2_mm_sec
        type: f4
      - id: retract_speed2_mm_sec
        type: f4
      - id: rest_time_after_lift_sec
        type: f4
      - id: machine_name_offset
        type: u4
      - id: machine_name_size
        type: u4
      - id: per_layer_settings
        type: u4
      - id: modified_timestamp_min_since_epoch
        type: u4
      - id: anti_alias_level
        type: u4
      - id: software_version
        type: u4
      - id: rest_time_after_retract_sec
        type: f4
      - id: rest_time_after_lift2_sec
        type: f4
      - id: transition_layer_count
        type: u4
      - id: print_settings_v4_offset
        type: u4
      - id: padding2
        type: u4
      - id: padding3
        type: u4
    instances:
      machine_name:
        pos: machine_name_offset
        size: machine_name_size
  layer:
    seq:
      - id: position_z_mm
        type: f4
      - id: exposure_time_sec
        type: f4
      - id: light_off_sec
        type: f4
      - id: image_offset
        type: u4
      - id: image_size
        type: u4
      - id: unknown1
        type: u4
      - id: table_size
        type: u4
      - id: unknown3
        type: u4
      - id: unknown4
        type: u4
    instances:
      image:
        pos: image_offset
        size: image_size
  preview:
    seq:
      - id: resolution_x
        type: u4
      - id: resolution_y
        type: u4
      - id: image_offset
        type: u4
      - id: image_size
        type: u4
      - id: unknown1
        type: u4
      - id: unknown2
        type: u4
      - id: unknown3
        type: u4
      - id: unknown4
        type: u4
    instances:
      image:
        pos: image_offset
        size: image_size
