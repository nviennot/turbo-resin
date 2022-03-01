# SPDX-License-Identifier: GPL-3.0-or-later

# Based on https://github.com/sn4k3/UVtools/blob/master/UVtools.Core/FileFormats/PhotonWorkshopFile.cs

meta:
  id: photon
  file-extension: pwma
  endian: le
seq:
  - id: header
    type: header
instances:
  config1:
    type: config1
    pos: header.header_offset
  preview:
    type: preview
    pos: header.preview_offset
  layer_definition:
    type: layer_definition
    pos: header.layer_definition_offset
  config2:
    type: config2
    pos: header.extra_offset
  machine:
    type: machine
    pos: header.machine_offset
types:
  header:
    seq:
      - id: magic
        contents: ['ANYCUBIC', 0, 0, 0, 0]
      - id: version
        type: u4
      - id: area_num
        type: u4
      - id: header_offset
        type: u4
      - id: padding1
        type: u4
      - id: preview_offset
        type: u4
      - id: preview_end_offset
        type: u4
      - id: layer_definition_offset
        type: u4
      - id: extra_offset
        type: u4
      - id: machine_offset
        type: u4
      - id: layer_image_offset
        type: u4
  config1:
    seq:
      - id: magic
        contents: ['HEADER', 0, 0, 0, 0, 0, 0]
      - id: length
        type: u4
      - id: pixel_size_um
        type: f4
      - id: layer_height
        type: f4
      - id: exposure_time
        type: f4
      - id: wait_time_before_cure
        type: f4
      - id: bottom_exposure_time
        type: f4
      - id: bottom_layers_count
        type: f4
      - id: lift_height
        type: f4
      - id: lift_speed
        type: f4
      - id: retract_speed
        type: f4
      - id: volume_ml
        type: f4
      - id: anti_aliasing
        type: u4
      - id: resolution_x
        type: u4
      - id: resolution_y
        type: u4
      - id: weight_g
        type: f4
      - id: price
        type: f4
      - id: price_currency
        type: u4
      - id: per_layer_override
        type: u4
      - id: print_time
        type: u4
      - id: transition_layer_count
        type: u4
      - id: padding
        type: u4
  preview:
    seq:
      - id: magic
        contents: ['PREVIEW', 0, 0, 0, 0, 0]
      - id: length
        type: u4
      - id: resolution_x
        type: u4
      - id: dpi
        type: u4
      - id: resolution_y
        type: u4
      - id: body
        size: resolution_x * resolution_y * 2
  layer_definition:
    seq:
      - id: magic
        contents: ['LAYERDEF', 0, 0, 0, 0]
      - id: length
        type: u4
      - id: layer_count
        type: u4
      - id: layers
        type: layer
        repeat: expr
        repeat-expr: layer_count
  layer:
    seq:
      - id: data_address
        type: u4
      - id: data_length
        type: u4
      - id: lift_height
        type: f4
      - id: lift_speed
        type: f4
      - id: exposure_time
        type: f4
      - id: layer_height
        type: f4
      - id: non_zero_pixel_count
        type: u4
      - id: padding
        type: u4
    instances:
      data:
        pos: data_address
        size: data_length
  config2:
    seq:
      - id: magic
        contents: ['EXTRA',0,0,0,0,0,0,0]
      - id: unknown1
        type: u4
      - id: unknown2
        type: u4
      - id: bottom_lift_height1
        type: f4
      - id: bottom_lift_speed1
        type: f4
      - id: bottom_retract_speed1
        type: f4
      - id: bottom_lift_height2
        type: f4
      - id: bottom_lift_speed2
        type: f4
      - id: bottom_retract_speed2
        type: f4
      - id: unknown3
        type: u4
      - id: lift_height1
        type: f4
      - id: lift_speed1
        type: f4
      - id: retract_speed1
        type: f4
      - id: lift_height2
        type: f4
      - id: lift_speed2
        type: f4
      - id: retract_speed2
        type: f4
  machine:
    seq:
      - id: magic
        contents: ['MACHINE',0,0,0,0,0]
      - id: length
        type: u4
      - id: name
        type: str
        size: 96
        encoding: utf8
      - id: layer_image_format
        type: str
        size: 24
        encoding: utf8
      - id: display_width
        type: f4
      - id: display_height
        type: f4
      - id: z_length_mm
        type: f4
      - id: version1
        type: u4
      - id: version2
        type: u4
