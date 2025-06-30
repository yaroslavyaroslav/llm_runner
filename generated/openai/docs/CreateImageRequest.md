# CreateImageRequest

## Properties

Name | Type | Description | Notes
------------ | ------------- | ------------- | -------------
**n** | Option<**i32**> | The number of images to generate. Must be between 1 and 10. | [optional]
**prompt** | **String** | A text description of the desired image(s). The maximum length is 1000 characters. | 
**response_format** | Option<**String**> | The format in which the generated images are returned. Must be one of `url` or `b64_json`. | [optional]
**size** | Option<**String**> | The size of the generated images. Must be one of `256x256`, `512x512`, or `1024x1024`. | [optional]
**user** | Option<**String**> | A unique identifier representing your end-user. | [optional]

[[Back to Model list]](../README.md#documentation-for-models) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to README]](../README.md)


