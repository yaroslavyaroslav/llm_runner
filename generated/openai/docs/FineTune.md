# FineTune

## Properties

Name | Type | Description | Notes
------------ | ------------- | ------------- | -------------
**created_at** | **i32** |  | 
**events** | Option<[**Vec<models::FineTuneEvent>**](FineTuneEvent.md)> |  | [optional]
**fine_tuned_model** | Option<**String**> |  | 
**hyperparams** | [**serde_json::Value**](.md) |  | 
**id** | **String** |  | 
**model** | **String** |  | 
**object** | **String** |  | 
**organization_id** | **String** |  | 
**result_files** | [**Vec<models::OpenAiFile>**](OpenAIFile.md) |  | 
**status** | **String** |  | 
**training_files** | [**Vec<models::OpenAiFile>**](OpenAIFile.md) |  | 
**updated_at** | **i32** |  | 
**validation_files** | [**Vec<models::OpenAiFile>**](OpenAIFile.md) |  | 

[[Back to Model list]](../README.md#documentation-for-models) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to README]](../README.md)


